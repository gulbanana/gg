use std::iter::Peekable;

use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local, LocalResult, TimeZone, Utc};
use futures_util::StreamExt;
use jj_lib::{
    backend::{CommitId, Timestamp},
    file_util::relative_path,
    matchers::EverythingMatcher,
    repo::Repo,
    revset::Revset,
    revset_graph::{RevsetGraphEdge, RevsetGraphEdgeType, TopoGroupedRevsetGraphIterator},
    rewrite::merge_commit_trees,
};
use pollster::FutureExt;

use crate::{
    gui_util::{SessionOperation, WorkspaceSession},
    messages::{
        DiffPath, DisplayPath, LogCoordinates, LogLine, LogPage, LogRow, RevDetail, RevHeader,
    },
};

const LOG_PAGE_SIZE: usize = 1000; // XXX configurable?

struct LogStem {
    source: LogCoordinates,
    target: CommitId,
    indirect: bool,
    was_inserted: bool,
}

pub struct LogQuery<'a> {
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
    pub fn new(revset: &dyn Revset) -> LogQuery {
        LogQuery {
            stems: Vec::new(),
            iter: TopoGroupedRevsetGraphIterator::new(revset.iter_graph()).peekable(),
            row: 0,
        }
    }

    pub fn get(&mut self, op: &SessionOperation) -> Result<LogPage> {
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
        Ok(LogPage {
            rows,
            has_more: self.iter.peek().is_some(),
        })
    }
}

pub fn query_revision(
    ws: &WorkspaceSession,
    op: &SessionOperation,
    id_str: &str,
) -> Result<RevDetail> {
    let id = CommitId::try_from_hex(id_str)?;
    let commit = op.repo.store().get_commit(&id)?;

    let parent_tree = merge_commit_trees(op.repo.as_ref(), &commit.parents())?;
    let tree = commit.tree()?;
    let mut tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);

    let mut paths = Vec::new();
    async {
        while let Some((repo_path, diff)) = tree_diff.next().await {
            let base_path = ws.workspace.workspace_root();
            let relative_path: DisplayPath =
                (&relative_path(base_path, &repo_path.to_fs_path(base_path))).into();
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

    let parents: Result<Vec<RevHeader>> = commit
        .parents()
        .iter()
        .map(|p| op.format_header(p))
        .collect();

    Ok(RevDetail {
        header: op.format_header(&commit)?,
        author: commit.author().name.clone(),
        email: commit.author().email.clone(),
        timestamp: datetime_from_timestamp(&commit.author().timestamp)
            .unwrap()
            .with_timezone(&Local),
        diff: paths,
        parents: parents?,
    })
}

// from time_util, which is not pub
pub fn datetime_from_timestamp(context: &Timestamp) -> Option<DateTime<FixedOffset>> {
    let utc = match Utc.timestamp_opt(
        context.timestamp.0.div_euclid(1000),
        (context.timestamp.0.rem_euclid(1000)) as u32 * 1000000,
    ) {
        LocalResult::None => {
            return None;
        }
        LocalResult::Single(x) => x,
        LocalResult::Ambiguous(y, _z) => y,
    };

    Some(
        utc.with_timezone(
            &FixedOffset::east_opt(context.tz_offset * 60)
                .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap()),
        ),
    )
}
