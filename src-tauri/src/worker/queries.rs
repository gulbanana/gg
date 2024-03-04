use std::iter::{Peekable, Skip};

use anyhow::Result;

use futures_util::StreamExt;
use jj_lib::{
    backend::CommitId,
    matchers::EverythingMatcher,
    revset::Revset,
    revset_graph::{RevsetGraphEdge, RevsetGraphEdgeType, TopoGroupedRevsetGraphIterator},
    rewrite::merge_commit_trees,
};
use pollster::FutureExt;

use crate::messages::{DiffPath, LogCoordinates, LogLine, LogPage, LogRow, RevDetail, RevHeader};

use super::WorkspaceSession;

struct LogStem {
    source: LogCoordinates,
    target: CommitId,
    indirect: bool,
    was_inserted: bool,
    known_immutable: bool,
}

/// state used for init or restart of a query
pub struct LogQueryState {
    /// max number of rows per page
    page_size: usize,
    /// number of rows already yielded
    next_row: usize,
    /// ongoing vertical lines; nodes will be placed on or around these
    stems: Vec<Option<LogStem>>,
}

impl LogQueryState {
    pub fn new(page_size: usize) -> LogQueryState {
        LogQueryState {
            page_size,
            next_row: 0,
            stems: Vec::new(),
        }
    }
}

/// live instance of a query
pub struct LogQuery<'a, 'b: 'a> {
    pub ws: &'a WorkspaceSession<'b>,
    iter: Peekable<
        Skip<
            TopoGroupedRevsetGraphIterator<
                Box<dyn Iterator<Item = (CommitId, Vec<RevsetGraphEdge>)> + 'a>,
            >,
        >,
    >,
    pub state: LogQueryState,
}

impl<'a, 'b> LogQuery<'a, 'b> {
    pub fn new(
        ws: &'a WorkspaceSession<'b>,
        revset: &'a dyn Revset,
        state: LogQueryState,
    ) -> LogQuery<'a, 'b> {
        let iter = TopoGroupedRevsetGraphIterator::new(revset.iter_graph())
            .skip(state.next_row)
            .peekable();

        LogQuery { ws, iter, state }
    }

    pub fn get_page(&mut self) -> Result<LogPage> {
        let mut rows: Vec<LogRow> = Vec::with_capacity(self.state.page_size); // output rows to draw
        let mut row = self.state.next_row;
        let max = row + self.state.page_size;

        while let Some((commit_id, commit_edges)) = self.iter.next() {
            // output lines to draw for the current row
            let mut lines: Vec<LogLine> = Vec::new();

            // find an existing stem targeting the current node
            let mut column = self.state.stems.len();
            let mut stem_known_immutable = false;
            let mut padding = 0; // used to offset the commit summary past some edges

            for (slot, stem) in self.state.stems.iter().enumerate() {
                if let Some(LogStem { target, .. }) = stem {
                    if *target == commit_id {
                        column = slot;
                        padding = self.state.stems.len() - column - 1;
                        break;
                    }
                }
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

            let header = self
                .ws
                .format_header(&self.ws.get_commit(&commit_id)?, stem_known_immutable)?;

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
            'edges: for edge in commit_edges.iter() {
                if edge.edge_type == RevsetGraphEdgeType::Missing {
                    continue;
                }

                for (slot, stem) in self.state.stems.iter().enumerate() {
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

                for stem in self.state.stems.iter_mut() {
                    if stem.is_none() {
                        *stem = Some(LogStem {
                            source: LogCoordinates(column, row),
                            target: edge.target.clone(),
                            indirect: edge.edge_type == RevsetGraphEdgeType::Indirect,
                            was_inserted: true,
                            known_immutable: header.is_immutable,
                        });
                        continue 'edges;
                    }
                }

                self.state.stems.push(Some(LogStem {
                    source: LogCoordinates(column, row),
                    target: edge.target.clone(),
                    indirect: edge.edge_type == RevsetGraphEdgeType::Indirect,
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
}

// XXX this is reloading the header, which the client already has
pub fn query_revision(ws: &WorkspaceSession, rev_str: &str) -> Result<RevDetail> {
    let commit = ws.evaluate_rev_str(rev_str)?;

    let parent_tree = merge_commit_trees(ws.repo(), &commit.parents())?;
    let tree = commit.tree()?;
    let mut tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);

    let mut paths = Vec::new();
    async {
        while let Some((repo_path, diff)) = tree_diff.next().await {
            let relative_path = ws.format_path(&repo_path);
            let (before, after) = diff.unwrap();

            if before.is_present() && after.is_present() {
                paths.push(DiffPath::Modified { relative_path });
            } else if before.is_absent() {
                paths.push(DiffPath::Added { relative_path });
            } else {
                paths.push(DiffPath::Deleted { relative_path });
            }
        }
    }
    .block_on();

    let header = ws.format_header(&commit, false)?;

    let parents: Result<Vec<RevHeader>> = commit
        .parents()
        .iter()
        .map(|p| ws.format_header(p, header.is_immutable))
        .collect();
    let parents = parents?;

    Ok(RevDetail {
        header,
        parents,
        diff: paths,
    })
}
