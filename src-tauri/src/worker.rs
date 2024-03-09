//! Worker per window, owning repo data (jj-lib is not thread-safe)

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use jj_lib::{
    backend::CommitId,
    file_util,
    matchers::EverythingMatcher,
    repo::Repo,
    revset_graph::{RevsetGraphEdgeType, TopoGroupedRevsetGraphIterator},
    rewrite::merge_commit_trees,
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
        tx: Sender<Result<messages::LogPage>>,
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
) -> Result<messages::LogPage> {
    let revset = eval
        .evaluate_revset(revset_str)
        .context("evaluate revset")?;

    let mut nodes = Vec::new();

    // XXX investigate paging for perf
    let iter = TopoGroupedRevsetGraphIterator::new(revset.iter_graph());

    // XXX if building the graph in JS is slow, we could do transformations here
    // the 1000 limit is temporary - we can load them at an ok speed, but that is too many dom nodes
    // to draw without virtualisation
    for (commit_id, commit_edges) in iter.take(1000) {
        let commit = op.repo.store().get_commit(&commit_id)?;
        let mut edges = Vec::new();

        for edge in commit_edges {
            match edge.edge_type {
                RevsetGraphEdgeType::Missing => {
                    edges.push(messages::LogEdge::Missing);
                }
                RevsetGraphEdgeType::Direct => {
                    edges.push(messages::LogEdge::Direct(op.format_commit_id(&edge.target)));
                }
                RevsetGraphEdgeType::Indirect => {
                    edges.push(messages::LogEdge::Indirect(
                        op.format_commit_id(&edge.target),
                    ));
                }
            }
        }

        nodes.push(messages::LogNode {
            revision: op.format_header(&commit)?,
            edges,
        });
    }

    Ok(messages::LogPage {
        nodes,
        has_more: false,
    })
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
        header: op.format_header(&commit)?,
        diff: paths,
    })
}
