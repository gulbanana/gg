//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker is organised as a matryoshka doll of state machines, each owning more session data than the one in which it is contained

use std::{
    iter::Peekable,
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
    revset::Revset,
    revset_graph::{RevsetGraphEdge, RevsetGraphEdgeType, TopoGroupedRevsetGraphIterator},
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
    QueryLogMore {
        tx: Sender<Result<messages::LogPage>>,
    },
    GetRevision {
        tx: Sender<Result<messages::RevDetail>>,
        rev: String,
    },
}

pub fn state_main(rx: Receiver<SessionEvent>) -> Result<()> {
    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenWorkspace {
                mut tx,
                cwd: mut wd,
            }) => loop {
                let cwd = &wd
                    .clone()
                    .unwrap_or_else(|| std::env::current_dir().unwrap());

                let session = match WorkspaceSession::from_cwd(cwd) {
                    Ok(session) => session,
                    Err(err) => {
                        tx.send(Ok(messages::RepoConfig::NoWorkspace {
                            absolute_path: cwd.into(),
                            error: format!("{err}"),
                        }))?;
                        continue;
                    }
                };

                let op = match SessionOperation::from_head(&session) {
                    Ok(op) => op,
                    Err(err) => {
                        tx.send(Ok(messages::RepoConfig::NoOperation {
                            absolute_path: cwd.into(),
                            error: format!("{err}"),
                        }))?;
                        continue;
                    }
                };

                let eval = SessionEvaluator::from_operation(&op);

                tx.send(Ok(op.format_config()))?;

                (tx, wd) = state_workspace(&rx, &session, &op, &eval)?;
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

type WorkspaceResult = (Sender<Result<messages::RepoConfig>>, Option<PathBuf>);

fn state_workspace(
    rx: &Receiver<SessionEvent>,
    session: &WorkspaceSession,
    op: &SessionOperation,
    eval: &SessionEvaluator,
) -> Result<WorkspaceResult> {
    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenWorkspace { tx, cwd }) => {
                return Ok((tx, cwd));
            }
            Ok(SessionEvent::QueryLog {
                mut tx,
                revset: mut revset_string,
            }) => loop {
                let revset = eval
                    .evaluate_revset(&revset_string)
                    .context("evaluate revset")?;
                let mut query = LogQuery::new(&*revset);
                let first_page = query.get(&op)?;
                let incomplete = first_page.has_more;
                tx.send(Ok(first_page))?;

                if incomplete {
                    match state_workspace_query(rx, session, &op, &eval, &mut query)? {
                        WorkspaceAndQueryResult::Workspace(r) => return Ok(r),
                        WorkspaceAndQueryResult::Requery(new_tx, new_revset_string) => {
                            (tx, revset_string) = (new_tx, new_revset_string)
                        }
                        WorkspaceAndQueryResult::QueryComplete => break,
                    };
                } else {
                    break;
                }
            },
            Ok(SessionEvent::QueryLogMore { tx: _tx }) => {
                return Err(anyhow::anyhow!("No log query is in progress"))
            }
            Ok(SessionEvent::GetRevision { tx, rev: rev_id }) => {
                tx.send(get_revision(&op, &rev_id))?
            }
            Err(err) => return Err(anyhow!(err)),
        };
    }
}

enum WorkspaceAndQueryResult {
    Workspace(WorkspaceResult),
    Requery(Sender<Result<messages::LogPage>>, String),
    QueryComplete,
}

fn state_workspace_query(
    rx: &Receiver<SessionEvent>,
    _session: &WorkspaceSession,
    op: &SessionOperation,
    _eval: &SessionEvaluator,
    query: &mut LogQuery,
) -> Result<WorkspaceAndQueryResult> {
    loop {
        match rx.recv() {
            Ok(SessionEvent::OpenWorkspace { tx, cwd }) => {
                return Ok(WorkspaceAndQueryResult::Workspace((tx, cwd)));
            }
            Ok(SessionEvent::QueryLog { tx, revset }) => {
                return Ok(WorkspaceAndQueryResult::Requery(tx, revset));
            }
            Ok(SessionEvent::QueryLogMore { tx }) => {
                let page = query.get(&op);
                let mut complete = false;
                tx.send(page.map(|p| {
                    if !p.has_more {
                        complete = true;
                    }
                    p
                }))?;
                if complete {
                    return Ok(WorkspaceAndQueryResult::QueryComplete);
                }
            }
            Ok(SessionEvent::GetRevision { tx, rev: rev_id }) => {
                tx.send(get_revision(&op, &rev_id))?
            }
            Err(err) => return Err(anyhow!(err)),
        };
    }
}

const LOG_PAGE_SIZE: usize = 1000; // XXX configurable?

struct LogStem {
    source: messages::LogCoordinates,
    target: CommitId,
    indirect: bool,
    was_inserted: bool,
}

struct LogQuery<'a> {
    /// ongoing vertical lines; nodes will be placed on or around these
    stems: Vec<Option<LogStem>>,
    iter: Peekable<
        TopoGroupedRevsetGraphIterator<
            Box<dyn Iterator<Item = (CommitId, Vec<RevsetGraphEdge>)> + 'a>,
        >,
    >,
    row: usize,
}

impl LogQuery<'_> {
    fn new(revset: &dyn Revset) -> LogQuery {
        LogQuery {
            stems: Vec::new(),
            iter: TopoGroupedRevsetGraphIterator::new(revset.iter_graph()).peekable(),
            row: 0,
        }
    }

    fn get(&mut self, op: &SessionOperation) -> Result<messages::LogPage> {
        // output rows to draw
        let mut rows: Vec<LogRow> = Vec::new();

        let mut row = self.row;
        let max = row + LOG_PAGE_SIZE;
        while let Some((commit_id, commit_edges)) = self.iter.next() {
            // output lines to draw for the current row
            let mut lines: Vec<LogLine> = Vec::new();

            // find an existing stem targeting the current node
            let mut column = self.stems.len();
            let mut padding = 0; // used to offset the commit summary past some edges

            for (slot, stem) in self.stems.iter().enumerate() {
                if let Some(LogStem { target, .. }) = stem {
                    if *target == commit_id {
                        column = slot;
                        padding = self.stems.len() - column - 1;
                        break;
                    }
                }
            }

            // terminate any existing stem, removing it from the end or leaving a gap
            if column < self.stems.len() {
                if let Some(terminated_stem) = &self.stems[column] {
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
                self.stems[column] = None;
            }
            // otherwise, slot into any gaps that might exist
            else {
                for (slot, stem) in self.stems.iter().enumerate() {
                    if stem.is_none() {
                        column = slot;
                        padding = self.stems.len() - slot - 1;
                        break;
                    }
                }
            }

            // remove empty stems on the right edge
            let empty_stems = self
                .stems
                .iter()
                .rev()
                .take_while(|stem| stem.is_none())
                .count();
            self.stems.truncate(self.stems.len() - empty_stems);

            // merge edges into existing stems or add new ones to the right
            'edges: for edge in commit_edges.iter() {
                if edge.edge_type == RevsetGraphEdgeType::Missing {
                    continue;
                }

                for (slot, stem) in self.stems.iter().enumerate() {
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

                for stem in self.stems.iter_mut() {
                    if stem.is_none() {
                        *stem = Some(LogStem {
                            source: LogCoordinates(column, row),
                            target: edge.target.clone(),
                            indirect: edge.edge_type == RevsetGraphEdgeType::Indirect,
                            was_inserted: true,
                        });
                        continue 'edges;
                    }
                }

                self.stems.push(Some(LogStem {
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

            row = row + 1;
            if row == max {
                break;
            }
        }

        self.row = row;
        Ok(messages::LogPage {
            rows,
            has_more: self.iter.peek().is_some(),
        })
    }
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
