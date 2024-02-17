//! Worker per window, owning repo data (jj-lib is not thread-safe)

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use jj_lib::{
    backend::CommitId, file_util, matchers::EverythingMatcher, repo::Repo,
    revset::RevsetIteratorExt, rewrite::merge_commit_trees,
};
use pollster::FutureExt;

use crate::{
    gui_util::{SessionEvaluator, SessionOperation, WorkspaceSession},
    messages,
};

#[derive(Debug)]
pub enum SessionEvent {
    OpenRepository {
        tx: Sender<Result<messages::RepoConfig>>,
        cwd: PathBuf,
    },
    QueryLog {
        tx: Sender<Result<Vec<messages::RevHeader>>>,
        revset: String,
    },
    GetRevision {
        tx: Sender<Result<messages::RevDetail>>,
        rev: String,
    },
}

pub fn main(rx: Receiver<SessionEvent>) -> Result<()> {
    let mut session;
    let mut op;
    let mut eval;

    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenRepository { tx, cwd }) => {
                tx.send({
                    session = WorkspaceSession::from_cwd(&cwd)?;
                    op = SessionOperation::from_head(&session)?;
                    eval = SessionEvaluator::from_operation(&op);
                    Ok(op.format_config())
                })?;
                break;
            }
            Ok(_) => {
                return Err(anyhow::anyhow!(
                    "A repo must be loaded before any other operations"
                ))
            }
            Err(err) => return Err(anyhow!(err)),
        };
    }

    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenRepository { tx, cwd }) => tx.send({
                drop(eval);
                session = WorkspaceSession::from_cwd(&cwd)?;
                op = SessionOperation::from_head(&session)?;
                eval = SessionEvaluator::from_operation(&op);
                Ok(op.format_config())
            })?,
            Ok(SessionEvent::QueryLog {
                tx,
                revset: rev_set,
            }) => tx.send(query_log(&op, &eval, &rev_set))?,
            Ok(SessionEvent::GetRevision { tx, rev: rev_id }) => {
                tx.send(get_revision(&op, &rev_id))?
            }
            Err(err) => return Err(anyhow!(err)),
        };
    }
}

fn query_log(
    op: &SessionOperation,
    eval: &SessionEvaluator,
    revset_str: &str,
) -> Result<Vec<messages::RevHeader>> {
    let revset = eval
        .evaluate_revset(revset_str)
        .context("evaluate revset")?;

    let mut output = Vec::new();
    for commit_or_error in revset.iter().commits(op.repo.store()) {
        let commit = commit_or_error?;
        output.push(op.format_header(&commit));
    }

    Ok(output)
}

fn get_revision(op: &SessionOperation, id_str: &str) -> Result<messages::RevDetail> {
    let id = CommitId::try_from_hex(id_str)?;
    let commit = op.repo.store().get_commit(&id)?;

    let parent_tree = merge_commit_trees(op.repo.as_ref(), &commit.parents())?;
    let tree = commit.tree()?;
    let mut tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);

    let mut paths = Vec::new();
    async {
        while let Some((repo_path, diff)) = tree_diff.next().await {
            let base_path = op.session.workspace.workspace_root();
            let relative_path: messages::DisplayPath =
                (&file_util::relative_path(base_path, &repo_path.to_fs_path(base_path))).into();
            let (before, after) = diff.unwrap();

            if before.is_present() && after.is_present() {
                paths.push(messages::DiffPath::Modified { relative_path });
            } else if before.is_absent() {
                paths.push(messages::DiffPath::Added { relative_path });
            } else {
                paths.push(messages::DiffPath::Deleted { relative_path });
            }
        }
    }
    .block_on();

    Ok(messages::RevDetail {
        header: op.format_header(&commit),
        diff: paths,
    })
}
