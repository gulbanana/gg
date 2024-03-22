use std::iter::{Peekable, Skip};

use anyhow::{anyhow, Result};

use futures_util::StreamExt;
use jj_lib::{
    backend::{BackendError, CommitId},
    matchers::EverythingMatcher,
    merged_tree::TreeDiffStream,
    repo::Repo,
    revset::Revset,
    revset_graph::{RevsetGraphEdge, RevsetGraphEdgeType, TopoGroupedRevsetGraphIterator},
    rewrite,
};
use pollster::FutureExt;

use crate::messages::{
    ChangeKind, LogCoordinates, LogLine, LogPage, LogRow, RevChange, RevHeader, RevId, RevResult,
    TreePath,
};

use super::WorkspaceSession;

struct LogStem {
    source: LogCoordinates,
    target: CommitId,
    indirect: bool,
    was_inserted: bool,
    known_immutable: bool,
}

/// state used for init or restart of a query
pub struct QueryState {
    /// max number of rows per page
    page_size: usize,
    /// number of rows already yielded
    next_row: usize,
    /// ongoing vertical lines; nodes will be placed on or around these
    stems: Vec<Option<LogStem>>,
}

impl QueryState {
    pub fn new(page_size: usize) -> QueryState {
        QueryState {
            page_size,
            next_row: 0,
            stems: Vec::new(),
        }
    }
}

/// live instance of a query
pub struct QuerySession<'a, 'b: 'a> {
    pub ws: &'a WorkspaceSession<'b>,
    iter: Peekable<
        Skip<
            TopoGroupedRevsetGraphIterator<
                Box<dyn Iterator<Item = (CommitId, Vec<RevsetGraphEdge>)> + 'a>,
            >,
        >,
    >,
    pub state: QueryState,
}

impl<'a, 'b> QuerySession<'a, 'b> {
    pub fn new(
        ws: &'a WorkspaceSession<'b>,
        revset: &'a dyn Revset,
        state: QueryState,
    ) -> QuerySession<'a, 'b> {
        let iter = TopoGroupedRevsetGraphIterator::new(revset.iter_graph())
            .skip(state.next_row)
            .peekable();

        QuerySession { ws, iter, state }
    }

    pub fn get_page(&mut self) -> Result<LogPage> {
        let mut rows: Vec<LogRow> = Vec::with_capacity(self.state.page_size); // output rows to draw
        let mut row = self.state.next_row;
        let max = row + self.state.page_size;
        let root_id = self.ws.repo().store().root_commit_id().clone();

        while let Some((commit_id, commit_edges)) = self.iter.next() {
            // output lines to draw for the current row
            let mut lines: Vec<LogLine> = Vec::new();

            // find an existing stem targeting the current node
            let mut column = self.state.stems.len();
            let mut stem_known_immutable = false;
            let mut padding = 0; // used to offset the commit summary past some edges

            if let Some(slot) = self.find_stem_for_commit(&commit_id) {
                column = slot;
                padding = self.state.stems.len() - column - 1;
            }

            // terminate any existing stem, removing it from the end or leaving a gap
            if column < self.state.stems.len() {
                if let Some(terminated_stem) = &self.state.stems[column] {
                    stem_known_immutable = terminated_stem.known_immutable;
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
                self.state.stems[column] = None;
            }
            // otherwise, slot into any gaps that might exist
            else {
                for (slot, stem) in self.state.stems.iter().enumerate() {
                    if stem.is_none() {
                        column = slot;
                        padding = self.state.stems.len() - slot - 1;
                        break;
                    }
                }
            }

            let known_immutable = if stem_known_immutable {
                Some(true)
            } else if !self.ws.should_check_immutable() {
                Some(false)
            } else {
                None
            };

            let header = self
                .ws
                .format_header(&self.ws.get_commit(&commit_id)?, known_immutable)?;

            // remove empty stems on the right edge
            let empty_stems = self
                .state
                .stems
                .iter()
                .rev()
                .take_while(|stem| stem.is_none())
                .count();
            self.state
                .stems
                .truncate(self.state.stems.len() - empty_stems);

            // merge edges into existing stems or add new ones to the right
            let mut next_missing: Option<CommitId> = None;
            'edges: for edge in commit_edges.iter() {
                if edge.edge_type == RevsetGraphEdgeType::Missing {
                    if edge.target == root_id {
                        continue;
                    } else {
                        next_missing = Some(edge.target.clone());
                    }
                }

                let indirect = edge.edge_type != RevsetGraphEdgeType::Direct;

                for (slot, stem) in self.state.stems.iter().enumerate() {
                    if let Some(stem) = stem {
                        if stem.target == edge.target {
                            lines.push(LogLine::ToIntersection {
                                indirect,
                                source: LogCoordinates(column, row),
                                target: LogCoordinates(slot, row + 1),
                            });
                            continue 'edges;
                        }
                    }
                }

                for stem in self.state.stems.iter_mut() {
                    if stem.is_none() {
                        *stem = Some(LogStem {
                            source: LogCoordinates(column, row),
                            target: edge.target.clone(),
                            indirect,
                            was_inserted: true,
                            known_immutable: header.is_immutable,
                        });
                        continue 'edges;
                    }
                }

                self.state.stems.push(Some(LogStem {
                    source: LogCoordinates(column, row),
                    target: edge.target.clone(),
                    indirect,
                    was_inserted: false,
                    known_immutable: header.is_immutable,
                }));
            }

            rows.push(LogRow {
                revision: header,
                location: LogCoordinates(column, row),
                padding,
                lines,
            });
            row = row + 1;

            // terminate any temporary stems created for missing edges
            match next_missing
                .take()
                .and_then(|id| self.find_stem_for_commit(&id))
            {
                Some(slot) => {
                    if let Some(terminated_stem) = &self.state.stems[slot] {
                        rows.last_mut().unwrap().lines.push(LogLine::ToMissing {
                            indirect: terminated_stem.indirect,
                            source: LogCoordinates(column, row - 1),
                            target: LogCoordinates(slot, row),
                        });
                    }
                    self.state.stems[slot] = None;
                    row = row + 1;
                }
                None => (),
            };

            if row == max {
                break;
            }
        }

        self.state.next_row = row;
        Ok(LogPage {
            rows,
            has_more: self.iter.peek().is_some(),
        })
    }

    fn find_stem_for_commit(&self, id: &CommitId) -> Option<usize> {
        for (slot, stem) in self.state.stems.iter().enumerate() {
            if let Some(LogStem { target, .. }) = stem {
                if target == id {
                    return Some(slot);
                }
            }
        }

        None
    }
}

// XXX this is reloading the header, which the client already has
pub fn query_revision(ws: &WorkspaceSession, id: RevId) -> Result<RevResult> {
    let commit = match ws.resolve_optional_id(&id)? {
        Some(commit) => commit,
        None => return Ok(RevResult::NotFound { id }),
    };

    let parent_tree = rewrite::merge_commit_trees(ws.repo(), &commit.parents())?;
    let tree = commit.tree()?;

    let mut conflicts: Vec<TreePath> = Vec::new();
    for (repo_path, entry) in parent_tree.entries() {
        if !entry.is_resolved() {
            conflicts.push(ws.format_path(repo_path));
        }
    }

    let mut changes = Vec::new();
    let tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);
    format_tree_changes(ws, &mut changes, tree_diff).block_on()?;

    let header = ws.format_header(&commit, None)?;

    let parents: Result<Vec<RevHeader>> = commit
        .parents()
        .iter()
        .map(|p| {
            ws.format_header(
                p,
                if header.is_immutable {
                    Some(true)
                } else {
                    None
                },
            )
        })
        .collect();
    let parents = parents?;

    Ok(RevResult::Detail {
        header,
        parents,
        changes,
        conflicts,
    })
}

pub fn query_remotes(
    ws: &WorkspaceSession,
    tracking_branch: Option<String>,
) -> Result<Vec<String>> {
    let git_repo = match ws.git_repo()? {
        Some(git_repo) => git_repo,
        None => return Err(anyhow!("No git backend")),
    };

    let all_remotes: Vec<String> = git_repo
        .remotes()?
        .into_iter()
        .filter_map(|remote| remote.map(|remote| remote.to_owned()))
        .collect();

    let matching_remotes = match tracking_branch {
        Some(branch_name) => all_remotes
            .into_iter()
            .filter(|remote_name| {
                let remote_ref = ws.view().get_remote_branch(&branch_name, &remote_name);
                !remote_ref.is_absent() && remote_ref.is_tracking()
            })
            .collect(),
        None => all_remotes,
    };

    Ok(matching_remotes)
}

async fn format_tree_changes(
    ws: &WorkspaceSession<'_>,
    changes: &mut Vec<RevChange>,
    mut tree_diff: TreeDiffStream<'_>,
) -> Result<(), BackendError> {
    while let Some((repo_path, entry)) = tree_diff.next().await {
        let (before, after) = entry?;
        changes.push(RevChange {
            path: ws.format_path(repo_path),
            kind: if before.is_present() && after.is_present() {
                ChangeKind::Modified
            } else if before.is_absent() {
                ChangeKind::Added
            } else {
                ChangeKind::Deleted
            },
            has_conflict: !after.is_resolved(),
        });
    }
    Ok(())
}
