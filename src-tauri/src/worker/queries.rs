use std::{
    io::Write,
    iter::{Peekable, Skip},
    ops::Range,
};

use anyhow::{anyhow, Result};

use futures_util::{try_join, StreamExt};
use gix::bstr::ByteVec;
use itertools::Itertools;
use jj_lib::{
    backend::CommitId,
    conflicts::{self, MaterializedTreeValue},
    diff::{self, Diff, DiffHunk},
    graph::{GraphEdge, GraphEdgeType, TopoGroupedGraphIterator},
    matchers::EverythingMatcher,
    merged_tree::TreeDiffStream,
    repo::Repo,
    repo_path::RepoPath,
    revset::{Revset, RevsetEvaluationError},
    rewrite,
};
use pollster::FutureExt;

use crate::messages::{
    ChangeHunk, ChangeKind, FileRange, HunkLocation, LogCoordinates, LogLine, LogPage, LogRow,
    MultilineString, RevChange, RevConflict, RevId, RevResult,
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
pub struct QuerySession<'q, 'w: 'q> {
    pub ws: &'q WorkspaceSession<'w>,
    pub state: QueryState,
    iter: Peekable<
        Skip<
            TopoGroupedGraphIterator<
                CommitId,
                Box<
                    dyn Iterator<
                            Item = Result<
                                (CommitId, Vec<GraphEdge<CommitId>>),
                                RevsetEvaluationError,
                            >,
                        > + 'q,
                >,
            >,
        >,
    >,
    is_immutable: Box<dyn Fn(&CommitId) -> Result<bool, RevsetEvaluationError> + 'q>,
}

impl<'q, 'w> QuerySession<'q, 'w> {
    pub fn new(
        ws: &'q WorkspaceSession<'w>,
        revset: &'q dyn Revset,
        state: QueryState,
    ) -> QuerySession<'q, 'w> {
        let iter = TopoGroupedGraphIterator::new(revset.iter_graph())
            .skip(state.next_row)
            .peekable();

        let immutable_revset = ws.evaluate_immutable().unwrap();
        let is_immutable = immutable_revset.containing_fn();

        QuerySession {
            ws,
            iter,
            state,
            is_immutable,
        }
    }

    pub fn get_page(&mut self) -> Result<LogPage> {
        let mut rows: Vec<LogRow> = Vec::with_capacity(self.state.page_size); // output rows to draw
        let mut row = self.state.next_row;
        let max = row + self.state.page_size;

        let root_id = self.ws.repo().store().root_commit_id().clone();

        while let Some(Ok((commit_id, commit_edges))) = self.iter.next() {
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
            } else {
                Some((self.is_immutable)(&commit_id)?)
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
                if edge.edge_type == GraphEdgeType::Missing {
                    if edge.target == root_id {
                        continue;
                    } else {
                        next_missing = Some(edge.target.clone());
                    }
                }

                let indirect = edge.edge_type != GraphEdgeType::Direct;

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

#[cfg(test)]
pub fn query_log(ws: &WorkspaceSession, revset_str: &str, max_results: usize) -> Result<LogPage> {
    let state = QueryState::new(max_results);
    let revset = ws.evaluate_revset_str(revset_str)?;
    let mut session = QuerySession::new(ws, &*revset, state);
    session.get_page()
}

// XXX this is reloading the header, which the client already has
pub fn query_revision(ws: &WorkspaceSession, id: RevId) -> Result<RevResult> {
    let commit = match ws.resolve_optional_id(&id)? {
        Some(commit) => commit,
        None => return Ok(RevResult::NotFound { id }),
    };

    let commit_parents: Result<Vec<_>, _> = commit.parents().collect();
    let parent_tree = rewrite::merge_commit_trees(ws.repo(), &commit_parents?)?;
    let tree = commit.tree()?;

    let mut conflicts = Vec::new();
    for (path, entry) in parent_tree.entries() {
        if let Ok(entry) = entry {
            if !entry.is_resolved() {
                match conflicts::materialize_tree_value(ws.repo().store(), &path, entry)
                    .block_on()?
                {
                    MaterializedTreeValue::Conflict { contents, .. } => {
                        let mut hunks = get_unified_hunks(3, &contents, &[])?;

                        if let Some(hunk) = hunks.pop() {
                            conflicts.push(RevConflict {
                                path: ws.format_path(path)?,
                                hunk,
                            });
                        }
                    }
                    _ => {
                        log::warn!("nonresolved tree entry did not materialise as conflict");
                    }
                }
            }
        }
    }

    let mut changes = Vec::new();
    let tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);
    format_tree_changes(ws, &mut changes, tree_diff).block_on()?;

    let header = ws.format_header(&commit, None)?;

    let parents = commit
        .parents()
        .map_ok(|p| {
            ws.format_header(
                &p,
                if header.is_immutable {
                    Some(true)
                } else {
                    None
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

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
                let remote_ref = ws.view().get_remote_bookmark(&branch_name, &remote_name);
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
) -> Result<()> {
    let store = ws.repo().store();

    while let Some((path, diff)) = tree_diff.next().await {
        let (before, after) = diff?;

        let kind = if before.is_present() && after.is_present() {
            ChangeKind::Modified
        } else if before.is_absent() {
            ChangeKind::Added
        } else {
            ChangeKind::Deleted
        };

        let has_conflict = !after.is_resolved();

        let before_future = conflicts::materialize_tree_value(store, &path, before);
        let after_future = conflicts::materialize_tree_value(store, &path, after);
        let (before_value, after_value) = try_join!(before_future, after_future)?;

        let hunks = get_value_hunks(3, &path, before_value, after_value)?;

        changes.push(RevChange {
            path: ws.format_path(path)?,
            kind,
            has_conflict,
            hunks,
        });
    }
    Ok(())
}

fn get_value_hunks(
    num_context_lines: usize,
    path: &RepoPath,
    left_value: MaterializedTreeValue,
    right_value: MaterializedTreeValue,
) -> Result<Vec<ChangeHunk>> {
    if left_value.is_absent() {
        let right_part = get_value_contents(path, right_value)?;
        get_unified_hunks(num_context_lines, &[], &right_part)
    } else if right_value.is_present() {
        let left_part = get_value_contents(&path, left_value)?;
        let right_part = get_value_contents(&path, right_value)?;
        get_unified_hunks(num_context_lines, &left_part, &right_part)
    } else {
        let left_part = get_value_contents(&path, left_value)?;
        get_unified_hunks(num_context_lines, &left_part, &[])
    }
}

fn get_value_contents(path: &RepoPath, value: MaterializedTreeValue) -> Result<Vec<u8>> {
    match value {
        MaterializedTreeValue::Absent => Err(anyhow!(
            "Absent path {path:?} in diff should have been handled by caller"
        )),
        MaterializedTreeValue::File { mut reader, .. } => {
            let mut contents = vec![];
            reader.read_to_end(&mut contents)?;

            let start = &contents[..8000.min(contents.len())]; // same heuristic git uses
            let is_binary = start.contains(&b'\0');
            if is_binary {
                contents.clear();
                contents.push_str("(binary)");
            }
            Ok(contents)
        }
        MaterializedTreeValue::Symlink { target, .. } => Ok(target.into_bytes()),
        MaterializedTreeValue::GitSubmodule(_) => Ok("(submodule)".to_owned().into_bytes()),
        MaterializedTreeValue::Conflict { contents, .. } => Ok(contents),
        MaterializedTreeValue::Tree(_) => Err(anyhow!("Unexpected tree in diff at path {path:?}")),
        MaterializedTreeValue::AccessDenied(error) => Err(anyhow!(error)),
    }
}

fn get_unified_hunks(
    num_context_lines: usize,
    left_content: &[u8],
    right_content: &[u8],
) -> Result<Vec<ChangeHunk>> {
    let mut hunks = Vec::new();

    for hunk in unified_diff_hunks(left_content, right_content, num_context_lines) {
        let location = HunkLocation {
            from_file: FileRange {
                start: hunk.left_line_range.start,
                len: hunk.left_line_range.len(),
            },
            to_file: FileRange {
                start: hunk.right_line_range.start,
                len: hunk.right_line_range.len(),
            },
        };

        let mut lines = Vec::new();
        for (line_type, content) in hunk.lines {
            let mut formatter: Vec<u8> = vec![];
            match line_type {
                DiffLineType::Context => {
                    write!(formatter, " ")?;
                }
                DiffLineType::Removed => {
                    write!(formatter, "-")?;
                }
                DiffLineType::Added => {
                    write!(formatter, "+")?;
                }
            }
            formatter.write(content)?;
            lines.push(std::str::from_utf8(&formatter)?.into());
        }

        hunks.push(ChangeHunk {
            location,
            lines: MultilineString { lines },
        });
    }

    Ok(hunks)
}

/**************************/
/* from jj_cli::diff_util */
/**************************/

#[derive(PartialEq)]
enum DiffLineType {
    Context,
    Removed,
    Added,
}

struct UnifiedDiffHunk<'content> {
    left_line_range: Range<usize>,
    right_line_range: Range<usize>,
    lines: Vec<(DiffLineType, &'content [u8])>,
}

fn unified_diff_hunks<'content>(
    left_content: &'content [u8],
    right_content: &'content [u8],
    num_context_lines: usize,
) -> Vec<UnifiedDiffHunk<'content>> {
    let mut hunks = vec![];
    let mut current_hunk = UnifiedDiffHunk {
        left_line_range: 1..1,
        right_line_range: 1..1,
        lines: vec![],
    };
    let mut show_context_after = false;
    let diff = Diff::for_tokenizer([left_content, right_content], &diff::find_line_ranges);
    for hunk in diff.hunks() {
        match hunk {
            DiffHunk::Matching(content) => {
                let lines = content.split_inclusive(|b| *b == b'\n').collect_vec();
                // Number of context lines to print after the previous non-matching hunk.
                let num_after_lines = lines.len().min(if show_context_after {
                    num_context_lines
                } else {
                    0
                });
                current_hunk.left_line_range.end += num_after_lines;
                current_hunk.right_line_range.end += num_after_lines;
                for line in lines.iter().take(num_after_lines) {
                    current_hunk.lines.push((DiffLineType::Context, line));
                }
                let num_skip_lines = lines
                    .len()
                    .saturating_sub(num_after_lines)
                    .saturating_sub(num_context_lines);
                if num_skip_lines > 0 {
                    let left_start = current_hunk.left_line_range.end + num_skip_lines;
                    let right_start = current_hunk.right_line_range.end + num_skip_lines;
                    if !current_hunk.lines.is_empty() {
                        hunks.push(current_hunk);
                    }
                    current_hunk = UnifiedDiffHunk {
                        left_line_range: left_start..left_start,
                        right_line_range: right_start..right_start,
                        lines: vec![],
                    };
                }
                let num_before_lines = lines.len() - num_after_lines - num_skip_lines;
                current_hunk.left_line_range.end += num_before_lines;
                current_hunk.right_line_range.end += num_before_lines;
                for line in lines.iter().skip(num_after_lines + num_skip_lines) {
                    current_hunk.lines.push((DiffLineType::Context, line));
                }
            }
            DiffHunk::Different(content) => {
                show_context_after = true;
                let left_lines = content[0].split_inclusive(|b| *b == b'\n').collect_vec();
                let right_lines = content[1].split_inclusive(|b| *b == b'\n').collect_vec();
                if !left_lines.is_empty() {
                    current_hunk.left_line_range.end += left_lines.len();
                    for line in left_lines {
                        current_hunk.lines.push((DiffLineType::Removed, line));
                    }
                }
                if !right_lines.is_empty() {
                    current_hunk.right_line_range.end += right_lines.len();
                    for line in right_lines {
                        current_hunk.lines.push((DiffLineType::Added, line));
                    }
                }
            }
        }
    }
    if !current_hunk
        .lines
        .iter()
        .all(|(diff_type, _line)| *diff_type == DiffLineType::Context)
    {
        hunks.push(current_hunk);
    }
    hunks
}
