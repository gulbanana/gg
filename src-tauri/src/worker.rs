//! Worker per window, owning repo data (jj-lib is not thread-safe)

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use jj_lib::{
    file_util, matchers::EverythingMatcher, repo::Repo, revset::RevsetIteratorExt,
    rewrite::merge_commit_trees,
};
use pollster::FutureExt;

use crate::{
    gui_util::{SessionOperation, WorkspaceSession},
    messages,
};

#[derive(Debug)]
pub enum SessionEvent {
    SetCwd {
        tx: Sender<Result<messages::WSStatus>>,
        cwd: PathBuf,
    },
    GetLog {
        tx: Sender<Result<Vec<messages::RevHeader>>>,
    },
    GetChange {
        tx: Sender<Result<messages::RevDetail>>,
        revision: String,
    },
}

pub fn main(rx: Receiver<SessionEvent>) -> Result<()> {
    let mut session = WorkspaceSession::from_cwd(&std::env::current_dir()?)?;
    let mut op = session.load_at_head()?;

    loop {
        match rx.recv() {
            Err(err) => return Err(anyhow!(err)),
            Ok(SessionEvent::SetCwd { tx, cwd }) => tx.send({
                session = WorkspaceSession::from_cwd(&cwd).context("load repo")?;
                op = session.load_at_head().context("load op head")?;
                Ok(op.format_status())
            })?,
            Ok(SessionEvent::GetLog { tx }) => tx.send(get_log(&op))?,
            Ok(SessionEvent::GetChange { tx, revision }) => tx.send(get_change(&op, revision))?,
        };
    }
}

fn get_log(op: &SessionOperation) -> Result<Vec<messages::RevHeader>> {
    let revset = op
        .evaluate_revset("..@ | ancestors(immutable_heads().., 2) | heads(immutable_heads())")
        .context("evaluate revset")?;

    let mut output = Vec::new();
    for commit_or_error in revset.iter().commits(op.repo.store()) {
        let commit = commit_or_error?;
        output.push(op.format_rev_header(&commit));
    }

    Ok(output)
}

fn get_change(op: &SessionOperation, revision_str: String) -> Result<messages::RevDetail> {
    let revset = op
        .evaluate_revset(&revision_str)
        .context("evaluate revset")?;

    let commit = revset
        .iter()
        .commits(op.repo.store())
        .next()
        .ok_or(anyhow!("commit not found"))??;

    let parent_tree = merge_commit_trees(op.repo.as_ref(), &commit.parents())?;
    let tree = commit.tree()?;
    let mut tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);

    let mut paths = Vec::new();
    async {
        while let Some((repo_path, diff)) = tree_diff.next().await {
            let base_path = op.session.workspace.workspace_root();
            let relative_path =
                file_util::relative_path(base_path, &repo_path.to_fs_path(base_path))
                    .to_string_lossy()
                    .into_owned();
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
        header: op.format_rev_header(&commit),
        diff: paths,
    })
}
