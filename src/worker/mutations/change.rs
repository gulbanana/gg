use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use itertools::Itertools;
use jj_lib::{
    backend::{CommitId, CopyId, FileId, TreeValue},
    commit::{Commit, conflict_label_for_commits},
    matchers::{EverythingMatcher, FilesMatcher, Matcher},
    merge::Merge,
    merged_tree::MergedTree,
    merged_tree_builder::MergedTreeBuilder,
    object_id::ObjectId as ObjectIdTrait,
    repo::Repo,
    repo_path::RepoPath,
    rewrite::{self, RebaseOptions, RebasedCommit},
};

use super::{precondition, read_file_content};

use crate::{
    messages::{
        ChangeHunk, TreePath,
        mutations::{
            CopyChanges, CopyHunk, MoveChanges, MoveHunk, MutationOptions, MutationResult,
        },
    },
    worker::{Mutation, gui_util::WorkspaceSession},
};

#[async_trait(?Send)]
impl Mutation for MoveChanges {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        // resolve & check destination
        let to_id = CommitId::try_from_hex(&self.to_id.hex).expect("frontend-validated id");
        if ws.check_immutable([to_id.clone()])? && !options.ignore_immutable {
            precondition!("Destination revision is immutable");
        }

        let mut to = ws.get_commit(&to_id)?;

        // resolve & check source
        let (from_commits, is_immutable) = ws.resolve_change_set(&self.from, true)?;
        if is_immutable && !options.ignore_immutable {
            precondition!("Some source revisions are immutable");
        }

        let from_newest = from_commits
            .first()
            .ok_or_else(|| anyhow!("empty revset"))?;
        let from_oldest = from_commits.last().ok_or_else(|| anyhow!("empty revset"))?;

        // construct trees: from_tree is newest state, parent_tree is base before oldest
        let matcher = build_matcher(&self.paths)?;
        let from_tree = from_newest.tree();
        let oldest_parents: Result<Vec<_>, _> = from_oldest.parents().collect();
        let oldest_parents = oldest_parents?;
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &oldest_parents).await?;
        let split_tree = rewrite::restore_tree(
            &from_tree,
            &parent_tree,
            from_newest.conflict_label(),
            conflict_label_for_commits(&oldest_parents),
            matcher.as_ref(),
        )
        .await?;

        // all sources will be abandoned if all changes in the range were selected
        let abandon_all = split_tree.tree_ids() == from_tree.tree_ids();

        // process each source commit: abandon, rewrite, or leave unchanged
        for commit in &from_commits {
            let commit_tree = commit.tree();
            let commit_parents: Result<Vec<_>, _> = commit.parents().collect();
            let commit_parents = commit_parents?;
            let commit_parent_tree =
                rewrite::merge_commit_trees(tx.repo(), &commit_parents).await?;
            let commit_remainder = rewrite::restore_tree(
                &commit_parent_tree,
                &commit_tree,
                conflict_label_for_commits(&commit_parents),
                commit.conflict_label(),
                matcher.as_ref(),
            )
            .await?;

            if commit_remainder.tree_ids() == commit_tree.tree_ids() {
                // commit didn't touch selected paths - leave unchanged
            } else if commit_remainder.tree_ids() == commit_parent_tree.tree_ids() {
                // commit only touched selected paths - abandon it
                tx.repo_mut().record_abandoned_commit(commit);
            } else {
                // commit touched both - rewrite with remaining changes
                tx.repo_mut()
                    .rewrite_commit(commit)
                    .set_tree(commit_remainder)
                    .write()?;
            }
        }

        // rebase descendants of source, which may include destination
        if tx.repo().index().is_ancestor(from_oldest.id(), to.id())? {
            let mut rebase_map = std::collections::HashMap::new();
            tx.repo_mut().rebase_descendants_with_options(
                &RebaseOptions::default(),
                |old_commit, rebased_commit| {
                    rebase_map.insert(
                        old_commit.id().clone(),
                        match rebased_commit {
                            RebasedCommit::Rewritten(new_commit) => new_commit.id().clone(),
                            RebasedCommit::Abandoned { parent_id } => parent_id,
                        },
                    );
                },
            )?;
            let rebased_to_id = rebase_map
                .get(to.id())
                .ok_or_else(|| anyhow!("descendant to_commit not found in rebase map"))?
                .clone();
            to = tx.repo().store().get_commit(&rebased_to_id)?;
        }

        // apply changes to destination
        let to_tree = to.tree();
        let new_to_tree = MergedTree::merge(Merge::from_vec(vec![
            (
                to_tree,
                format!("{} (move destination)", to.conflict_label()),
            ),
            (
                parent_tree,
                format!(
                    "{} (parents of moved revision)",
                    from_oldest.conflict_label()
                ),
            ),
            (
                split_tree,
                format!("{} (moved changes)", from_newest.conflict_label()),
            ),
        ]))
        .await?;
        let source_refs: Vec<_> = from_commits.iter().collect();
        let description = combine_messages(&source_refs, &to, abandon_all);
        tx.repo_mut()
            .rewrite_commit(&to)
            .set_tree(new_to_tree)
            .set_description(description)
            .write()?;

        match ws.finish_transaction(
            tx,
            format!(
                "move changes from {}::{} to {}",
                from_oldest.id().hex(),
                from_newest.id().hex(),
                to.id().hex()
            ),
        )? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for CopyChanges {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let from = ws.resolve_commit_id(&self.from_id)?;
        let from_tree = from.tree();
        let matcher = build_matcher(&self.paths)?;

        let (commits, is_immutable) = ws.resolve_change_set(&self.to_set, true)?;
        if is_immutable && !options.ignore_immutable {
            if commits.len() == 1 {
                precondition!("Destination revision is immutable");
            } else {
                precondition!("Some destination revisions are immutable");
            }
        }

        if commits.is_empty() {
            return Ok(MutationResult::Unchanged);
        }

        // walk up the range, replacing the specified changes to eliminate each revision's contribution to the combined diff
        let mut any_changed = false;
        for commit in commits.iter().rev() {
            let to_tree = commit.tree();
            let new_tree = rewrite::restore_tree(
                &from_tree,
                &to_tree,
                from.conflict_label(),
                commit.conflict_label(),
                matcher.as_ref(),
            )
            .await?;

            if new_tree.tree_ids() != to_tree.tree_ids() {
                any_changed = true;
                tx.repo_mut()
                    .rewrite_commit(commit)
                    .set_tree(new_tree)
                    .write()?;
            }
        }

        if !any_changed {
            return Ok(MutationResult::Unchanged);
        }

        tx.repo_mut().rebase_descendants()?;

        let description = if commits.len() == 1 {
            format!("restore into commit {}", commits[0].id().hex())
        } else {
            format!("restore into {} commits", commits.len())
        };

        match ws.finish_transaction(tx, description)? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for MoveHunk {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let from = ws.resolve_change_id(&self.from_id)?;
        let mut to = ws.resolve_commit_id(&self.to_id)?;

        if ws.check_immutable(vec![from.id().clone(), to.id().clone()])?
            && !options.ignore_immutable
        {
            precondition!("Some revisions are immutable");
        }

        // Split-rebase-squash algorithm:
        // - sibling_tree represents a virtual commit with just the hunk (like jj split)
        // - from_tree is modified by extracting the hunk, and its descendants updated (like jj rebase)
        // - to_tree is given the added hunk by doing a 3-way merge (like jj squash)
        let mut tx: jj_lib::transaction::Transaction = ws.start_transaction().await?;
        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;

        // Get the base tree (from's parent) - this is the tree the hunk was computed against
        let from_tree = from.tree();
        let from_parents: Result<Vec<_>, _> = from.parents().collect();
        let from_parents = from_parents?;
        if from_parents.len() != 1 {
            precondition!("Cannot move hunk from a merge commit");
        }
        let base_tree = from_parents[0].tree();

        // Construct the "sibling tree": base_tree with just this hunk applied.
        // This represents a virtual sibling commit containing only the hunk.
        let store = tx.repo().store();
        let base_content = read_file_content(store, &base_tree, repo_path).await?;
        let sibling_content = apply_hunk_to_base(&base_content, &self.hunk)?;
        let sibling_blob_id = store
            .write_file(repo_path, &mut sibling_content.as_slice())
            .await?;
        let sibling_executable = match from_tree.path_value(repo_path)?.into_resolved() {
            Ok(Some(TreeValue::File { executable, .. })) => executable,
            Ok(_) => false,
            Err(_) => false,
        };
        let sibling_tree = update_tree_entry(
            store,
            &base_tree,
            repo_path,
            sibling_blob_id,
            sibling_executable,
        )?;

        // Remove hunk from source: backout the base→sibling diff from from_tree
        let remainder_tree = MergedTree::merge(Merge::from_vec(vec![
            (
                from_tree.clone(),
                format!("{} (hunk source)", from.conflict_label()),
            ),
            (
                sibling_tree.clone(),
                format!("{} (moved hunk)", from.conflict_label()),
            ),
            (
                base_tree.clone(),
                format!(
                    "{} (parent of hunk source)",
                    from_parents[0].conflict_label()
                ),
            ),
        ]))
        .await?;

        // Apply hunk to destination: merge the base→sibling diff into to_tree
        // (may be recomputed after rebase in the from_is_ancestor case)
        let to_tree = to.tree();
        let mut new_to_tree = MergedTree::merge(Merge::from_vec(vec![
            (
                to_tree,
                format!("{} (hunk destination)", to.conflict_label()),
            ),
            (
                base_tree.clone(),
                format!(
                    "{} (parent of hunk source)",
                    from_parents[0].conflict_label()
                ),
            ),
            (
                sibling_tree.clone(),
                format!("{} (moved hunk)", from.conflict_label()),
            ),
        ]))
        .await?;

        let abandon_source = remainder_tree.tree_ids() == base_tree.tree_ids();
        let description = combine_messages(&[&from], &to, abandon_source);

        // Check ancestry to determine rebase strategy. The hunk must be applied to the destination's
        // tree AFTER any ancestry-related rebasing, so we do it early if moving from an ancestor.
        let from_is_ancestor = tx.repo().index().is_ancestor(from.id(), to.id())?;
        let to_is_ancestor = tx.repo().index().is_ancestor(to.id(), from.id())?;

        if to_is_ancestor {
            // Child→Parent: apply hunk to ancestor, then handle source
            tx.repo_mut()
                .rewrite_commit(&to)
                .set_tree(new_to_tree)
                .set_description(description)
                .write()?;

            if abandon_source {
                tx.repo_mut().record_abandoned_commit(&from);
            } else {
                tx.repo_mut()
                    .rewrite_commit(&from)
                    .set_tree(remainder_tree)
                    .write()?;
            }

            // Rebase all descendants, which includes rebasing source's descendants onto modified ancestor
            tx.repo_mut().rebase_descendants()?;
        } else {
            // Parent→Child or Unrelated: modify source first
            if abandon_source {
                tx.repo_mut().record_abandoned_commit(&from);
            } else {
                tx.repo_mut()
                    .rewrite_commit(&from)
                    .set_tree(remainder_tree)
                    .write()?;
            }

            if from_is_ancestor {
                // Parent→Child: rebase descendants first, then apply hunk to the rebased destination
                let mut rebase_map = std::collections::HashMap::new();
                tx.repo_mut().rebase_descendants_with_options(
                    &RebaseOptions::default(),
                    |old_commit, rebased_commit| {
                        rebase_map.insert(
                            old_commit.id().clone(),
                            match rebased_commit {
                                RebasedCommit::Rewritten(new_commit) => new_commit.id().clone(),
                                RebasedCommit::Abandoned { parent_id } => parent_id,
                            },
                        );
                    },
                )?;

                // The destination was rebased onto the modified source, so its tree changed.
                // Recompute the hunk application against the rebased tree.
                let rebased_to_id = rebase_map
                    .get(to.id())
                    .ok_or_else(|| anyhow!("descendant to_commit not found in rebase map"))?
                    .clone();
                to = tx.repo().store().get_commit(&rebased_to_id)?;
                new_to_tree = MergedTree::merge(Merge::from_vec(vec![
                    (
                        to.tree(),
                        format!("{} (rebased hunk destination)", to.conflict_label()),
                    ),
                    (
                        base_tree.clone(),
                        format!(
                            "{} (parent of hunk source)",
                            from_parents[0].conflict_label()
                        ),
                    ),
                    (
                        sibling_tree.clone(),
                        format!("{} (moved hunk)", from.conflict_label()),
                    ),
                ]))
                .await?;
            }

            // Apply hunk to destination
            tx.repo_mut()
                .rewrite_commit(&to)
                .set_tree(new_to_tree)
                .set_description(description)
                .write()?;

            // Rebase all descendants as usual
            tx.repo_mut().rebase_descendants()?;
        }

        match ws.finish_transaction(
            tx,
            format!(
                "move hunk in {} from {} to {}",
                self.path.repo_path,
                from.id().hex(),
                to.id().hex()
            ),
        )? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for CopyHunk {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let from = ws.resolve_commit_id(&self.from_id)?;
        let to = ws.resolve_change_id(&self.to_id)?;
        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;

        if ws.check_immutable(vec![to.id().clone()])? && !options.ignore_immutable {
            precondition!("Revision is immutable");
        }

        let store = tx.repo().store();
        let to_tree = to.tree();

        // vheck for conflicts in destination
        let to_path_value = to_tree.path_value(repo_path)?;
        if to_path_value.into_resolved().is_err() {
            precondition!("Cannot restore hunk: destination file has conflicts");
        }

        // read destination content
        let to_content = read_file_content(store, &to_tree, repo_path).await?;
        let to_text = String::from_utf8_lossy(&to_content);
        let to_lines: Vec<&str> = to_text.lines().collect();

        // validate destination bounds
        let to_start_0based = self.hunk.location.to_file.start.saturating_sub(1);
        let to_end_0based = to_start_0based + self.hunk.location.to_file.len;
        if to_end_0based > to_lines.len() {
            precondition!(
                "Hunk location out of bounds: file has {} lines, hunk requires lines {}-{}",
                to_lines.len(),
                self.hunk.location.to_file.start,
                to_end_0based
            );
        }

        // validate destination content
        let expected_to_lines: Vec<&str> = self
            .hunk
            .lines
            .lines
            .iter()
            .filter(|line| line.starts_with(' ') || line.starts_with('+'))
            .map(|line| line[1..].trim_end())
            .collect();
        let actual_to_lines: Vec<&str> = to_lines[to_start_0based..to_end_0based]
            .iter()
            .map(|line| line.trim_end())
            .collect();

        if expected_to_lines.len() != actual_to_lines.len() {
            return Err(anyhow!(
                "Hunk validation failed: expected {} lines, found {} lines at destination",
                expected_to_lines.len(),
                actual_to_lines.len()
            ));
        }

        for (i, (expected, actual)) in expected_to_lines
            .iter()
            .zip(actual_to_lines.iter())
            .enumerate()
        {
            if expected != actual {
                return Err(anyhow!(
                    "Hunk validation failed at line {}: expected '{}', found '{}'",
                    to_start_0based + i + 1,
                    expected,
                    actual
                ));
            }
        }

        // read source content
        let from_tree = from.tree();
        let from_content = read_file_content(store, &from_tree, repo_path).await?;
        let from_text = String::from_utf8_lossy(&from_content);
        let from_lines: Vec<&str> = from_text.lines().collect();

        // validate source bounds
        let from_start_0based = self.hunk.location.from_file.start.saturating_sub(1);
        let from_end_0based = from_start_0based + self.hunk.location.from_file.len;
        if from_end_0based > from_lines.len() {
            precondition!(
                "Source hunk location out of bounds: file has {} lines, hunk requires lines {}-{}",
                from_lines.len(),
                self.hunk.location.from_file.start,
                from_end_0based
            );
        }

        // extract source region
        let source_region_lines = &from_lines[from_start_0based..from_end_0based];

        // construct destination content and check whether anything changed
        let mut new_to_lines = Vec::new();
        new_to_lines.extend(to_lines[..to_start_0based].iter().map(|s| s.to_string()));
        new_to_lines.extend(source_region_lines.iter().map(|s| s.to_string()));
        new_to_lines.extend(to_lines[to_end_0based..].iter().map(|s| s.to_string()));

        let ends_with_newline = to_content.ends_with(b"\n");
        let mut new_to_content = Vec::new();
        let num_lines = new_to_lines.len();
        for (i, line) in new_to_lines.iter().enumerate() {
            new_to_content.extend_from_slice(line.as_bytes());
            if i < num_lines - 1 {
                new_to_content.push(b'\n');
            }
        }
        if ends_with_newline && !new_to_content.is_empty() && !new_to_content.ends_with(b"\n") {
            new_to_content.push(b'\n');
        }

        if new_to_content == to_content {
            return Ok(MutationResult::Unchanged);
        }

        // create new destination tree with preserved executable bit
        let new_to_blob_id = store
            .write_file(repo_path, &mut new_to_content.as_slice())
            .await?;

        let to_executable = match to_tree.path_value(repo_path)?.into_resolved() {
            Ok(Some(TreeValue::File { executable, .. })) => executable,
            _ => false,
        };

        let new_to_tree =
            update_tree_entry(store, &to_tree, repo_path, new_to_blob_id, to_executable)?;

        // rewrite destination
        tx.repo_mut()
            .rewrite_commit(&to)
            .set_tree(new_to_tree)
            .write()?;

        tx.repo_mut().rebase_descendants()?;

        match ws.finish_transaction(
            tx,
            format!(
                "restore hunk in {} from {} into {}",
                self.path.repo_path, self.from_id.hex, self.to_id.commit.hex
            ),
        )? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

/// Construct a sibling tree's file content by applying a hunk to its base.
///
/// The hunk was computed as a diff between `base` (the source commit's parent) and the
/// source commit. This function applies that diff to reconstruct the file content that
/// would exist in a virtual "sibling" commit containing only this hunk.
///
/// Line numbers must match exactly since the hunk was computed against this base.
#[allow(clippy::manual_strip)]
fn apply_hunk_to_base(base_content: &[u8], hunk: &ChangeHunk) -> Result<Vec<u8>> {
    let base_text = String::from_utf8_lossy(base_content);
    let base_lines: Vec<&str> = base_text.lines().collect();
    let ends_with_newline = base_content.ends_with(b"\n");

    let mut result_lines: Vec<String> = Vec::new();
    let hunk_lines = hunk.lines.lines.iter().peekable();

    // Convert 1-indexed line number to 0-indexed
    let hunk_start = hunk.location.from_file.start.saturating_sub(1);

    // Copy lines before the hunk unchanged
    result_lines.extend(base_lines[..hunk_start].iter().map(|s| s.to_string()));
    let mut base_idx = hunk_start;

    for diff_line in hunk_lines {
        if diff_line.starts_with(' ') || diff_line.starts_with('-') {
            // Context or deletion: verify the base content matches
            let expected = &diff_line[1..];
            if base_idx < base_lines.len() && base_lines[base_idx].trim_end() == expected.trim_end()
            {
                if diff_line.starts_with(' ') {
                    result_lines.push(base_lines[base_idx].to_string());
                }
                // Deletions are consumed but not added to result
                base_idx += 1;
            } else {
                anyhow::bail!(
                    "Hunk mismatch at line {}: expected '{}', found '{}'",
                    base_idx + 1,
                    expected.trim_end(),
                    base_lines.get(base_idx).map_or("<EOF>", |l| l.trim_end())
                );
            }
        } else if diff_line.starts_with('+') {
            // Addition: include in result
            let added = diff_line[1..].trim_end_matches('\n');
            result_lines.push(added.to_string());
        } else {
            anyhow::bail!("Malformed diff line: {}", diff_line);
        }
    }

    // Copy remaining lines after the hunk unchanged
    result_lines.extend(base_lines[base_idx..].iter().map(|s| s.to_string()));

    let mut result_bytes = Vec::new();
    let num_lines = result_lines.len();
    for (i, line) in result_lines.iter().enumerate() {
        result_bytes.extend_from_slice(line.as_bytes());
        if i < num_lines - 1 {
            result_bytes.push(b'\n');
        }
    }

    if ends_with_newline && !result_bytes.is_empty() && !result_bytes.ends_with(b"\n") {
        result_bytes.push(b'\n');
    }

    Ok(result_bytes)
}

fn build_matcher(paths: &[TreePath]) -> Result<Box<dyn Matcher>> {
    if paths.is_empty() {
        Ok(Box::new(EverythingMatcher))
    } else {
        let repo_paths: Vec<_> = paths
            .iter()
            .map(|p| RepoPath::from_internal_string(&p.repo_path))
            .try_collect()?;
        Ok(Box::new(FilesMatcher::new(&repo_paths)))
    }
}

fn combine_messages(sources: &[&Commit], destination: &Commit, abandon_source: bool) -> String {
    if abandon_source {
        // collect non-empty descriptions: destination first, then sources (newest to oldest)
        let descriptions: Vec<_> = std::iter::once(destination.description())
            .chain(sources.iter().map(|c| c.description()))
            .filter(|d| !d.is_empty())
            .collect();
        descriptions.join("\n")
    } else {
        destination.description().to_owned()
    }
}

fn update_tree_entry(
    _store: &Arc<jj_lib::store::Store>,
    original_tree: &MergedTree,
    path: &RepoPath,
    new_blob: FileId,
    executable: bool,
) -> Result<MergedTree, anyhow::Error> {
    let mut builder = MergedTreeBuilder::new(original_tree.clone());
    builder.set_or_remove(
        path.to_owned(),
        Merge::normal(TreeValue::File {
            id: new_blob,
            executable,
            copy_id: CopyId::placeholder(),
        }),
    );
    let new_tree = builder.write_tree()?;
    Ok(new_tree)
}
