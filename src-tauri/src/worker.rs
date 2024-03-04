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
    messages::{self, LogCoordinates, LogLine, LogRow},
};

#[derive(Debug)]
pub enum SessionEvent {
    OpenWorkspace {
        tx: Sender<Result<messages::RepoConfig>>,
        cwd: Option<PathBuf>,
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

pub fn without_workspace(rx: Receiver<SessionEvent>) -> Result<()> {
    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenWorkspace { mut tx, mut cwd }) => loop {
                if let Some((new_tx, new_cwd)) = with_workspace(
                    &cwd.unwrap_or_else(|| std::env::current_dir().unwrap()),
                    &rx,
                    tx,
                )? {
                    (tx, cwd) = (new_tx, new_cwd); // open succeeded and a new open was subsequently requested
                } else {
                    break; // open failed, wait for another attempt
                }
            },
            Ok(_) => {
                return Err(anyhow::anyhow!(
                    "A repo must be loaded before any other operations"
                ))
            }
            Err(err) => return Err(anyhow!(err)),
        };
    }
}

fn with_workspace(
    cwd: &PathBuf,
    rx: &Receiver<SessionEvent>,
    tx: Sender<Result<messages::RepoConfig>>,
) -> Result<Option<(Sender<Result<messages::RepoConfig>>, Option<PathBuf>)>> {
    let session;
    let op;
    let eval;

    match WorkspaceSession::from_cwd(cwd) {
        Ok(ok) => session = ok,
        Err(err) => {
            tx.send(Ok(messages::RepoConfig::NoWorkspace {
                absolute_path: cwd.into(),
                error: format!("{err}"),
            }))?;
            return Ok(None);
        }
    };

    match SessionOperation::from_head(&session) {
        Ok(ok) => op = ok,
        Err(err) => {
            tx.send(Ok(messages::RepoConfig::NoOperation {
                absolute_path: cwd.into(),
                error: format!("{err}"),
            }))?;
            return Ok(None);
        }
    };

    eval = SessionEvaluator::from_operation(&op);

    tx.send(Ok(op.format_config()))?;

    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenWorkspace { tx, cwd }) => {
                return Ok(Some((tx, cwd)));
            }
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

struct GraphStem {
    source: messages::LogCoordinates,
    target: CommitId,
    indirect: bool,
    was_inserted: bool,
}

fn query_log(
    op: &SessionOperation,
    eval: &SessionEvaluator,
    revset_str: &str,
) -> Result<messages::LogPage> {
    let revset = eval
        .evaluate_revset(revset_str)
        .context("evaluate revset")?;

    // ongoing vertical lines; nodes will be placed on or around these
    let mut stems: Vec<Option<GraphStem>> = Vec::new();

    // output rows to draw
    let mut rows: Vec<LogRow> = Vec::new();

    // XXX investigate paging for perf
    let iter = TopoGroupedRevsetGraphIterator::new(revset.iter_graph());
    for (row, (commit_id, commit_edges)) in iter.enumerate() {
        // output lines to draw for the current row
        let mut lines: Vec<LogLine> = Vec::new();

        // find an existing stem targeting the current node
        let mut column = stems.len();
        let mut padding = 0; // used to offset the commit summary past some edges

        for (slot, stem) in stems.iter().enumerate() {
            if let Some(GraphStem { target, .. }) = stem {
                if *target == commit_id {
                    column = slot;
                    padding = stems.len() - column - 1;
                    break;
                }
            }
        }

        // terminate any existing stem, removing it from the end or leaving a gap
        if column < stems.len() {
            if let Some(terminated_stem) = &stems[column] {
                lines.push(if terminated_stem.was_inserted {
                    LogLine::FromNode {
                        indirect: terminated_stem.indirect,
                        source: terminated_stem.source,
                        target: LogCoordinates(column, row),
                    }
                } else {
                    LogLine::ToNode {
                        indirect: terminated_stem.indirect,
                        source: terminated_stem.source,
                        target: LogCoordinates(column, row),
                    }
                });
            }
            stems[column] = None;
        }
        // otherwise, slot into any gaps that might exist
        else {
            for (slot, stem) in stems.iter().enumerate() {
                if stem.is_none() {
                    column = slot;
                    padding = stems.len() - slot - 1;
                    break;
                }
            }
        }

        // remove empty stems on the right edge
        let empty_stems = stems.iter().rev().take_while(|stem| stem.is_none()).count();
        stems.truncate(stems.len() - empty_stems);

        // merge edges into existing stems or add new ones to the right
        'edges: for edge in commit_edges.iter() {
            if edge.edge_type == RevsetGraphEdgeType::Missing {
                continue;
            }

            for (slot, stem) in stems.iter().enumerate() {
                if let Some(stem) = stem {
                    if stem.target == edge.target {
                        lines.push(LogLine::ToIntersection {
                            indirect: edge.edge_type == RevsetGraphEdgeType::Indirect,
                            source: LogCoordinates(column, row),
                            target: LogCoordinates(slot, row + 1),
                        });
                        continue 'edges;
                    }
                }
            }

            for stem in stems.iter_mut() {
                if stem.is_none() {
                    *stem = Some(GraphStem {
                        source: LogCoordinates(column, row),
                        target: edge.target.clone(),
                        indirect: edge.edge_type == RevsetGraphEdgeType::Indirect,
                        was_inserted: true,
                    });
                    continue 'edges;
                }
            }

            stems.push(Some(GraphStem {
                source: LogCoordinates(column, row),
                target: edge.target.clone(),
                indirect: edge.edge_type == RevsetGraphEdgeType::Indirect,
                was_inserted: false,
            }));
        }

        rows.push(LogRow {
            revision: op.format_header(&op.repo.store().get_commit(&commit_id)?)?,
            location: LogCoordinates(column, row),
            padding,
            lines,
        });
    }

    Ok(messages::LogPage {
        rows,
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

    let parents: Result<Vec<messages::RevHeader>> = commit
        .parents()
        .iter()
        .map(|p| op.format_header(p))
        .collect();

    Ok(messages::RevDetail {
        header: op.format_header(&commit)?,
        diff: paths,
        parents: parents?,
    })
}
