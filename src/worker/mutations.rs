use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    backend::{BackendError, CommitId, CopyId, FileId, TreeValue},
    commit::{Commit, conflict_label_for_commits},
    conflicts::{self, ConflictMarkerStyle, ConflictMaterializeOptions, MaterializedTreeValue},
    files::FileMergeHunkLevel,
    git::{
        self, GitBranchPushTargets, GitSettings, GitSubprocessOptions,
        REMOTE_NAME_FOR_LOCAL_GIT_REPO,
    },
    matchers::{EverythingMatcher, FilesMatcher, Matcher},
    merge::{Merge, SameChange},
    merged_tree::{MergedTree, MergedTreeBuilder},
    object_id::ObjectId as ObjectIdTrait,
    op_store::{RefTarget, RemoteRef, RemoteRefState},
    op_walk,
    ref_name::{RefNameBuf, RemoteName, RemoteNameBuf, RemoteRefSymbol},
    refs::{self, BookmarkPushAction, BookmarkPushUpdate, LocalAndRemoteRef},
    repo::Repo,
    repo_path::RepoPath,
    revset::{self, RevsetIteratorExt},
    rewrite::{self, RebaseOptions, RebasedCommit},
    settings::UserSettings,
    store::Store,
    str_util::{StringExpression, StringPattern},
    tree_merge::MergeOptions,
};
use tokio::io::AsyncReadExt;

use crate::git_util::AuthContext;
use crate::messages::{
    AbandonRevisions, AdoptRevision, BackoutRevisions, CheckoutRevision, CopyChanges, CopyHunk,
    CreateRef, CreateRevision, CreateRevisionBetween, DeleteRef, DescribeRevision,
    DuplicateRevisions, GitFetch, GitPush, GitRefspec, InsertRevisions, MoveChanges, MoveHunk,
    MoveRef, MoveRevisions, MutationResult, RenameBookmark, StoreRef, TrackBookmark, TreePath,
    UndoOperation, UntrackBookmark,
};

use super::Mutation;
use super::gui_util::{WorkspaceSession, get_git_remote_names, load_git_import_options};

macro_rules! precondition {
    ($($args:tt)*) => {
        return Ok(MutationResult::PreconditionError { message: format!($($args)*) })
    }
}

#[async_trait(?Send)]
impl Mutation for AbandonRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (commits, is_immutable) = ws.resolve_change_set(&self.set, true)?;
        if is_immutable {
            if commits.len() == 1 {
                precondition!("Revision is immutable");
            } else {
                precondition!("Some revisions are immutable");
            }
        }

        if commits.is_empty() {
            return Ok(MutationResult::Unchanged);
        }

        for commit in &commits {
            tx.repo_mut().record_abandoned_commit(commit);
        }
        tx.repo_mut().rebase_descendants()?;

        let transaction_description = if commits.len() == 1 {
            format!("abandon commit {}", commits[0].id().hex())
        } else {
            format!(
                "abandon commit {} and {} more",
                commits[0].id().hex(),
                commits.len() - 1
            )
        };

        match ws.finish_transaction(tx, transaction_description)? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for BackoutRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (commits, _) = ws.resolve_change_set(&self.set, false)?;

        if commits.is_empty() {
            return Ok(MutationResult::Unchanged);
        }

        // parent_tree: the base of all changes
        let oldest = commits.last().ok_or_else(|| anyhow!("empty revset"))?;
        let oldest_parents: Result<Vec<_>, BackendError> = oldest.parents().collect();
        let oldest_parents = oldest_parents?;
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &oldest_parents).await?;

        // reverted_tree: parent_tree + the changes we're backing out
        let newest = commits.first().ok_or_else(|| anyhow!("empty revset"))?;
        let reverted_tree = newest.tree();

        // wc_tree: contents of the working copy before we add to it
        let working_copy = ws.get_commit(ws.wc_id())?;
        let wc_tree = working_copy.tree();

        // prepare conflict labels
        let (reverted_label, parent_label) = if commits.len() == 1 {
            (
                format!("{} (backed out revision)", newest.conflict_label()),
                format!(
                    "{} (parents of backed out revision)",
                    conflict_label_for_commits(&oldest_parents)
                ),
            )
        } else {
            (
                format!(
                    "{}..{} (backed out revisions)",
                    oldest.conflict_label(),
                    newest.conflict_label()
                ),
                format!(
                    "{} (parents of backed out revisions)",
                    conflict_label_for_commits(&oldest_parents)
                ),
            )
        };
        let wc_label = format!("{} (backout destination)", working_copy.conflict_label());

        // three-way merge: wc + (reverted - parent)
        let new_wc_tree = MergedTree::merge(Merge::from_vec(vec![
            (wc_tree, wc_label),
            (reverted_tree, reverted_label),
            (parent_tree, parent_label),
        ]))
        .await?;

        tx.repo_mut()
            .rewrite_commit(&working_copy)
            .set_tree(new_wc_tree)
            .write()?;

        let transaction_description = if commits.len() == 1 {
            format!("back out commit {}", newest.id().hex())
        } else {
            format!(
                "back out commit {} and {} more",
                newest.id().hex(),
                commits.len() - 1
            )
        };

        match ws.finish_transaction(tx, transaction_description)? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for CheckoutRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let edited = ws.resolve_change_id(&self.id)?;

        if ws.check_immutable(vec![edited.id().clone()])? {
            precondition!("Revision is immutable");
        }

        if edited.id() == ws.wc_id() {
            return Ok(MutationResult::Unchanged);
        }

        tx.repo_mut().edit(ws.name().to_owned(), &edited)?;

        match ws.finish_transaction(tx, format!("edit commit {}", edited.id().hex()))? {
            Some(new_status) => {
                let new_selection = Some(ws.format_header(&edited, Some(false))?);
                Ok(MutationResult::Updated {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for CreateRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (parent_commits, _) = ws.resolve_change_set(&self.set, false)?;

        // use as parents of new revision
        let parent_ids: Vec<_> = parent_commits.iter().map(Commit::id).cloned().collect();
        let merged_tree = rewrite::merge_commit_trees(tx.repo(), &parent_commits).await?;
        let new_commit = tx.repo_mut().new_commit(parent_ids, merged_tree).write()?;

        // make it the working copy
        tx.repo_mut().edit(ws.name().to_owned(), &new_commit)?;

        match ws.finish_transaction(tx, "new empty commit")? {
            Some(new_status) => {
                let new_selection = Some(ws.format_header(&new_commit, Some(false))?);
                Ok(MutationResult::Updated {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for CreateRevisionBetween {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let parent_id = ws
            .resolve_commit_id(&self.after_id)
            .context("resolve after_id")?;
        let parent_ids = vec![parent_id.id().clone()];
        let parent_commits = vec![parent_id];
        let merged_tree = rewrite::merge_commit_trees(tx.repo(), &parent_commits).await?;

        let new_commit = tx.repo_mut().new_commit(parent_ids, merged_tree).write()?;

        let before_commit = ws
            .resolve_change_id(&self.before_id)
            .context("resolve before_id")?;
        if ws.check_immutable(vec![before_commit.id().clone()])? {
            precondition!("'Before' revision is immutable");
        }

        rewrite::rebase_commit(tx.repo_mut(), before_commit, vec![new_commit.id().clone()]).await?;

        tx.repo_mut().edit(ws.name().to_owned(), &new_commit)?;

        match ws.finish_transaction(tx, "new empty commit")? {
            Some(new_status) => {
                let new_selection = Some(ws.format_header(&new_commit, Some(false))?);
                Ok(MutationResult::Updated {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for DescribeRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let described = ws.resolve_change_id(&self.id)?;

        if ws.check_immutable(vec![described.id().clone()])? {
            precondition!("Revision {} is immutable", self.id.change.prefix);
        }

        if self.new_description == described.description() && !self.reset_author {
            return Ok(MutationResult::Unchanged);
        }

        let mut commit_builder = tx
            .repo_mut()
            .rewrite_commit(&described)
            .set_description(self.new_description);

        if self.reset_author {
            let new_author = commit_builder.committer().clone();
            commit_builder = commit_builder.set_author(new_author);
        }

        commit_builder.write()?;

        match ws.finish_transaction(tx, format!("describe commit {}", described.id().hex()))? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for DuplicateRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (clonees, _) = ws.resolve_change_set(&self.set, false)?;
        let num_clonees = clonees.len();
        let mut clones: IndexMap<Commit, Commit> = IndexMap::new();

        // toposort ensures that parents are duplicated first
        for clonee in clonees.into_iter().rev() {
            let clone_parents: Result<Vec<_>, _> = clonee
                .parents()
                .map_ok(|parent| {
                    if let Some(cloned_parent) = clones.get(&parent) {
                        cloned_parent
                    } else {
                        &parent
                    }
                    .id()
                    .clone()
                })
                .collect();
            let clone = tx
                .repo_mut()
                .rewrite_commit(&clonee)
                .clear_rewrite_source()
                .generate_new_change_id()
                .set_parents(clone_parents?)
                .write()?;
            clones.insert(clonee, clone);
        }

        match ws.finish_transaction(tx, format!("duplicating {} commit(s)", num_clonees))? {
            Some(new_status) => {
                if num_clonees == 1 {
                    let new_commit = clones
                        .get_index(0)
                        .ok_or(anyhow!("single source should have single copy"))?
                        .1;
                    let new_selection = Some(ws.format_header(new_commit, None)?);
                    Ok(MutationResult::Updated {
                        new_status,
                        new_selection,
                    })
                } else {
                    Ok(MutationResult::Updated {
                        new_status,
                        new_selection: None,
                    })
                }
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for InsertRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (targets, is_immutable) = ws.resolve_change_set(&self.set, true)?;
        if is_immutable {
            if targets.len() == 1 {
                precondition!("Revision is immutable");
            } else {
                precondition!("Some revisions are immutable");
            }
        }

        if targets.is_empty() {
            return Ok(MutationResult::Unchanged);
        }

        let before = ws
            .resolve_change_id(&self.before_id)
            .context("resolve before_id")?;
        let after = ws
            .resolve_change_id(&self.after_id)
            .context("resolve after_id")?;

        if ws.check_immutable([before.id().clone()])? {
            precondition!("Before revision is immutable");
        }

        // detach external children of any commit in the range
        let oldest = targets.last().expect("non-empty targets");
        let orphan_to = oldest.parent_ids().to_vec();
        let rebased_children = ws.disinherit_children(&mut tx, &targets, orphan_to).await?;

        // update after, which may have been a descendant of the range
        let after_id = rebased_children
            .get(after.id())
            .unwrap_or(after.id())
            .clone();

        // rebase the oldest onto after; the rest of the range follows
        let transaction_description = if targets.len() == 1 {
            format!("insert commit {}", oldest.id().hex())
        } else {
            format!("insert {} commits", targets.len())
        };
        let rebased_oldest =
            rewrite::rebase_commit(tx.repo_mut(), oldest.clone(), vec![after_id]).await?;

        // newest commit may have been rebased, so find its new ID
        let newest = targets.first().expect("non-empty targets");
        let new_newest_id = if targets.len() == 1 {
            rebased_oldest.id().clone()
        } else {
            let mut mapping = HashMap::new();
            tx.repo_mut().rebase_descendants_with_options(
                &RebaseOptions::default(),
                |old_commit, rebased| {
                    mapping.insert(
                        old_commit.id().clone(),
                        match rebased {
                            RebasedCommit::Rewritten(new_commit) => new_commit.id().clone(),
                            RebasedCommit::Abandoned { parent_id } => parent_id,
                        },
                    );
                },
            )?;
            mapping
                .get(newest.id())
                .cloned()
                .unwrap_or_else(|| newest.id().clone())
        };

        // rebase graph suffix onto the end of the inserted range
        rewrite::rebase_commit(tx.repo_mut(), before, vec![new_newest_id]).await?;

        match ws.finish_transaction(tx, transaction_description)? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for MoveRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (targets, is_immutable) = ws.resolve_change_set(&self.set, true)?;
        if is_immutable {
            if targets.len() == 1 {
                precondition!("Revision is immutable");
            } else {
                precondition!("Some revisions are immutable");
            }
        }

        if targets.is_empty() {
            return Ok(MutationResult::Unchanged);
        }

        // detach external children of any commit in the range
        let oldest = targets.last().expect("non-empty targets");
        let orphan_to = oldest.parent_ids().to_vec();
        let rebased_children = ws.disinherit_children(&mut tx, &targets, orphan_to).await?;

        // update parents, which may have been descendants of the targets
        let parents = ws.resolve_multiple_changes(self.parent_ids)?;
        let parent_ids: Vec<_> = parents
            .iter()
            .map(|new_parent| {
                rebased_children
                    .get(new_parent.id())
                    .unwrap_or(new_parent.id())
                    .clone()
            })
            .collect();
        let transaction_description = if targets.len() == 1 {
            format!("rebase commit {}", oldest.id().hex())
        } else {
            format!("rebase {} commits", targets.len())
        };
        rewrite::rebase_commit(tx.repo_mut(), oldest.clone(), parent_ids).await?;

        match ws.finish_transaction(tx, transaction_description)? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for AdoptRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let target = ws.resolve_change_id(&self.id)?;
        let parent_ids: Vec<_> = ws
            .resolve_multiple_commits(&self.parent_ids)?
            .into_iter()
            .map(|commit| commit.id().clone())
            .collect();

        // check for duplicate parents
        let unique_count = parent_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        if unique_count != parent_ids.len() {
            precondition!("Duplicate parent IDs");
        }

        if ws.check_immutable(vec![target.id().clone()])? {
            precondition!("Revision {} is immutable", self.id.change.prefix);
        }

        // just rebase the target, which will also rebase its descendants
        let rebased_id = target.id().hex();
        rewrite::rebase_commit(tx.repo_mut(), target, parent_ids).await?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for MoveChanges {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        // resolve & check destination
        let to_id = CommitId::try_from_hex(&self.to_id.hex).expect("frontend-validated id");
        if ws.check_immutable([to_id.clone()])? {
            precondition!("Destination revision is immutable");
        }

        let mut to = ws.get_commit(&to_id)?;

        // resolve & check source
        let (from_commits, is_immutable) = ws.resolve_change_set(&self.from, true)?;
        if is_immutable {
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
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &oldest_parents?).await?;
        let split_tree = rewrite::restore_tree(&from_tree, &parent_tree, matcher.as_ref()).await?;

        // all sources will be abandoned if all changes in the range were selected
        let abandon_all = split_tree.tree_ids() == from_tree.tree_ids();

        // process each source commit: abandon, rewrite, or leave unchanged
        for commit in &from_commits {
            let commit_tree = commit.tree();
            let commit_parents: Result<Vec<_>, _> = commit.parents().collect();
            let commit_parent_tree =
                rewrite::merge_commit_trees(tx.repo(), &commit_parents?).await?;
            let commit_remainder =
                rewrite::restore_tree(&commit_parent_tree, &commit_tree, matcher.as_ref()).await?;

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
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let from_tree = ws.resolve_commit_id(&self.from_id)?.tree();
        let matcher = build_matcher(&self.paths)?;

        let (commits, is_immutable) = ws.resolve_change_set(&self.to_set, true)?;
        if is_immutable {
            if commits.len() == 1 {
                precondition!("Destination revision is immutable");
            } else {
                precondition!("Some destination revisions are immutable");
            }
        }

        if commits.is_empty() {
            return Ok(MutationResult::Unchanged);
        }

        // process commits oldest-first (resolve_multiple returns newest-first)
        let mut any_changed = false;
        for commit in commits.iter().rev() {
            let to_tree = commit.tree();
            let new_tree = rewrite::restore_tree(&from_tree, &to_tree, matcher.as_ref()).await?;

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
impl Mutation for TrackBookmark {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        match self.r#ref {
            StoreRef::Tag { tag_name } => {
                precondition!("{} is a tag and cannot be tracked", tag_name);
            }
            StoreRef::LocalBookmark { bookmark_name, .. } => {
                precondition!(
                    "{} is a local bookmark and cannot be tracked",
                    bookmark_name
                );
            }
            StoreRef::RemoteBookmark {
                bookmark_name,
                remote_name,
                ..
            } => {
                let mut tx = ws.start_transaction().await?;
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &bookmark_name_ref,
                    remote: &remote_name_ref,
                };

                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_bookmark(remote_ref_symbol);

                if remote_ref.is_tracked() {
                    precondition!(
                        "{:?}@{:?} is already tracked",
                        bookmark_name_ref.as_str(),
                        remote_name_ref.as_str()
                    );
                }

                tx.repo_mut().track_remote_bookmark(remote_ref_symbol)?;

                match ws.finish_transaction(
                    tx,
                    format!(
                        "track remote bookmark {:?}@{:?}",
                        bookmark_name_ref.as_str(),
                        remote_name_ref.as_str()
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
    }
}

#[async_trait(?Send)]
impl Mutation for UntrackBookmark {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let mut untracked = Vec::new();
        match self.r#ref {
            StoreRef::Tag { tag_name } => {
                precondition!("{} is a tag and cannot be untracked", tag_name);
            }
            StoreRef::LocalBookmark { bookmark_name, .. } => {
                // untrack all remotes
                for (remote_ref_symbol, remote_ref) in ws.view().remote_bookmarks_matching(
                    &StringPattern::exact(bookmark_name).to_matcher(),
                    &StringPattern::all().to_matcher(),
                ) {
                    if remote_ref_symbol.remote != REMOTE_NAME_FOR_LOCAL_GIT_REPO
                        && remote_ref.is_tracked()
                    {
                        tx.repo_mut().untrack_remote_bookmark(remote_ref_symbol);
                        untracked.push(format!(
                            "{}@{}",
                            remote_ref_symbol.name.as_str(),
                            remote_ref_symbol.remote.as_str()
                        ));
                    }
                }
            }
            StoreRef::RemoteBookmark {
                bookmark_name,
                remote_name,
                ..
            } => {
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &bookmark_name_ref,
                    remote: &remote_name_ref,
                };
                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_bookmark(remote_ref_symbol);

                if !remote_ref.is_tracked() {
                    precondition!(
                        "{:?}@{:?} is not tracked",
                        bookmark_name_ref.as_str(),
                        remote_name_ref.as_str()
                    );
                }

                tx.repo_mut().untrack_remote_bookmark(remote_ref_symbol);
                untracked.push(format!(
                    "{}@{}",
                    bookmark_name_ref.as_str(),
                    remote_name_ref.as_str()
                ));
            }
        }

        match ws.finish_transaction(
            tx,
            format!("untrack remote {}", combine_bookmarks(&untracked)),
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
impl Mutation for RenameBookmark {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let old_name = self.r#ref.as_bookmark()?;
        let old_name_ref = RefNameBuf::from(old_name);

        let ref_target = ws.view().get_local_bookmark(&old_name_ref).clone();
        if ref_target.is_absent() {
            precondition!("No such bookmark: {}", old_name_ref.as_str());
        }

        let new_name_ref = RefNameBuf::from(self.new_name);
        if ws.view().get_local_bookmark(&new_name_ref).is_present() {
            precondition!("Bookmark already exists: {}", new_name_ref.as_str());
        }

        let mut tx = ws.start_transaction().await?;

        tx.repo_mut()
            .set_local_bookmark_target(&new_name_ref, ref_target);
        tx.repo_mut()
            .set_local_bookmark_target(&old_name_ref, RefTarget::absent());

        match ws.finish_transaction(
            tx,
            format!(
                "rename {} to {}",
                old_name_ref.as_str(),
                new_name_ref.as_str()
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
impl Mutation for CreateRef {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let commit = ws.resolve_change_id(&self.id)?;

        match self.r#ref {
            StoreRef::RemoteBookmark {
                bookmark_name,
                remote_name,
                ..
            } => {
                precondition!(
                    "{}@{} is a remote bookmark and cannot be created",
                    bookmark_name,
                    remote_name
                );
            }
            StoreRef::LocalBookmark { bookmark_name, .. } => {
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let existing_bookmark = ws.view().get_local_bookmark(&bookmark_name_ref);
                if existing_bookmark.is_present() {
                    precondition!("{} already exists", bookmark_name_ref.as_str());
                }

                tx.repo_mut().set_local_bookmark_target(
                    &bookmark_name_ref,
                    RefTarget::normal(commit.id().clone()),
                );

                match ws.finish_transaction(
                    tx,
                    format!(
                        "create {} pointing to commit {}",
                        bookmark_name_ref.as_str(),
                        ws.format_commit_id(commit.id()).hex
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated {
                        new_status,
                        new_selection: None,
                    }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name, .. } => {
                let tag_name_ref = RefNameBuf::from(tag_name);
                let existing_tag = ws.view().get_local_tag(&tag_name_ref);
                if existing_tag.is_present() {
                    precondition!("{} already exists", tag_name_ref.as_str());
                }

                tx.repo_mut()
                    .set_local_tag_target(&tag_name_ref, RefTarget::normal(commit.id().clone()));

                match ws.finish_transaction(
                    tx,
                    format!(
                        "create {} pointing to commit {}",
                        tag_name_ref.as_str(),
                        ws.format_commit_id(commit.id()).hex
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
    }
}

#[async_trait(?Send)]
impl Mutation for DeleteRef {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        match self.r#ref {
            StoreRef::RemoteBookmark {
                bookmark_name,
                remote_name,
                ..
            } => {
                let mut tx = ws.start_transaction().await?;

                // forget the bookmark entirely - when target is absent, it's removed from the view
                let remote_ref = RemoteRef {
                    target: RefTarget::absent(),
                    state: RemoteRefState::New,
                };
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &bookmark_name_ref,
                    remote: &remote_name_ref,
                };

                tx.repo_mut()
                    .set_remote_bookmark(remote_ref_symbol, remote_ref);

                match ws.finish_transaction(
                    tx,
                    format!(
                        "forget {}@{}",
                        bookmark_name_ref.as_str(),
                        remote_name_ref.as_str()
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated {
                        new_status,
                        new_selection: None,
                    }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::LocalBookmark { bookmark_name, .. } => {
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let mut tx = ws.start_transaction().await?;

                tx.repo_mut()
                    .set_local_bookmark_target(&bookmark_name_ref, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget {}", bookmark_name_ref.as_str()))? {
                    Some(new_status) => Ok(MutationResult::Updated {
                        new_status,
                        new_selection: None,
                    }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name } => {
                let tag_name_ref = RefNameBuf::from(tag_name);
                let mut tx = ws.start_transaction().await?;

                tx.repo_mut()
                    .set_local_tag_target(&tag_name_ref, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget tag {}", tag_name_ref.as_str()))? {
                    Some(new_status) => Ok(MutationResult::Updated {
                        new_status,
                        new_selection: None,
                    }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

// does not currently enforce fast-forwards
#[async_trait(?Send)]
impl Mutation for MoveRef {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let commit = ws.resolve_change_id(&self.to_id)?;

        match self.r#ref {
            StoreRef::RemoteBookmark {
                bookmark_name,
                remote_name,
                ..
            } => {
                precondition!("Bookmark is remote: {bookmark_name}@{remote_name}")
            }
            StoreRef::LocalBookmark { bookmark_name, .. } => {
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let old_target = ws.view().get_local_bookmark(&bookmark_name_ref);
                if old_target.is_absent() {
                    precondition!("No such bookmark: {:?}", bookmark_name_ref.as_str());
                }

                tx.repo_mut().set_local_bookmark_target(
                    &bookmark_name_ref,
                    RefTarget::normal(commit.id().clone()),
                );

                match ws.finish_transaction(
                    tx,
                    format!(
                        "point {:?} to commit {}",
                        &bookmark_name_ref,
                        commit.id().hex()
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated {
                        new_status,
                        new_selection: None,
                    }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name } => {
                let tag_name_ref = RefNameBuf::from(tag_name);
                let old_target = ws.view().get_local_tag(&tag_name_ref);
                if old_target.is_absent() {
                    precondition!("No such tag: {:?}", tag_name_ref.as_str());
                }

                tx.repo_mut()
                    .set_local_tag_target(&tag_name_ref, RefTarget::normal(commit.id().clone()));

                match ws.finish_transaction(
                    tx,
                    format!(
                        "point {:?} to commit {}",
                        tag_name_ref.as_str(),
                        commit.id().hex()
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
    }
}

#[async_trait(?Send)]
impl Mutation for MoveHunk {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let from = ws.resolve_change_id(&self.from_id)?;
        let mut to = ws.resolve_commit_id(&self.to_id)?;

        if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
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
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let from = ws.resolve_commit_id(&self.from_id)?;
        let to = ws.resolve_change_id(&self.to_id)?;
        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;

        if ws.check_immutable(vec![to.id().clone()])? {
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

#[async_trait(?Send)]
impl Mutation for GitPush {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        // determine bookmarks to push, recording the old and new commits
        let mut remote_bookmark_updates: Vec<(&str, Vec<(RefNameBuf, refs::BookmarkPushUpdate)>)> =
            Vec::new();
        let remote_bookmark_refs: Vec<_> = match &self.refspec {
            GitRefspec::AllBookmarks { remote_name } => {
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let mut bookmark_updates = Vec::new();
                for (bookmark_name, targets) in ws.view().local_remote_bookmarks(&remote_name_ref) {
                    if !targets.remote_ref.is_tracked() {
                        continue;
                    }

                    match classify_bookmark_push(bookmark_name.as_str(), remote_name, targets) {
                        Err(message) => return Ok(MutationResult::PreconditionError { message }),
                        Ok(None) => (),
                        Ok(Some(update)) => {
                            bookmark_updates.push((bookmark_name.to_owned(), update))
                        }
                    }
                }
                remote_bookmark_updates.push((remote_name, bookmark_updates));

                ws.view()
                    .remote_bookmarks(&remote_name_ref)
                    .map(|(name, remote_ref)| (name.to_owned(), remote_ref))
                    .collect()
            }
            GitRefspec::AllRemotes { bookmark_ref } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);

                let mut remote_bookmark_refs = Vec::new();
                for (remote_name, group) in ws
                    .view()
                    .all_remote_bookmarks()
                    .filter_map(|(remote_ref_symbol, remote_ref)| {
                        if remote_ref.is_tracked()
                            && remote_ref_symbol.name == bookmark_name_ref
                            && remote_ref_symbol.remote != REMOTE_NAME_FOR_LOCAL_GIT_REPO
                        {
                            Some((remote_ref_symbol.remote, remote_ref))
                        } else {
                            None
                        }
                    })
                    .chunk_by(|(remote_name, _)| *remote_name)
                    .into_iter()
                {
                    let mut bookmark_updates = Vec::new();
                    for (_, remote_ref) in group {
                        let targets = LocalAndRemoteRef {
                            local_target: ws.view().get_local_bookmark(&bookmark_name_ref),
                            remote_ref,
                        };
                        match classify_bookmark_push(bookmark_name, remote_name.as_str(), targets) {
                            Err(message) => {
                                return Ok(MutationResult::PreconditionError { message });
                            }
                            Ok(None) => (),
                            Ok(Some(update)) => {
                                bookmark_updates.push((RefNameBuf::from(bookmark_name), update))
                            }
                        }
                        remote_bookmark_refs.push((RefNameBuf::from(bookmark_name), remote_ref));
                    }
                    remote_bookmark_updates.push((remote_name.as_str(), bookmark_updates));
                }

                remote_bookmark_refs
            }
            GitRefspec::RemoteBookmark {
                remote_name,
                bookmark_ref,
            } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let local_target = ws.view().get_local_bookmark(&bookmark_name_ref);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &bookmark_name_ref,
                    remote: &remote_name_ref,
                };
                let remote_ref = ws.view().get_remote_bookmark(remote_ref_symbol);

                match classify_bookmark_push(
                    bookmark_name,
                    remote_name,
                    LocalAndRemoteRef {
                        local_target,
                        remote_ref,
                    },
                ) {
                    Err(message) => return Ok(MutationResult::PreconditionError { message }),
                    Ok(None) => (),
                    Ok(Some(update)) => {
                        remote_bookmark_updates
                            .push((remote_name, vec![(RefNameBuf::from(bookmark_name), update)]));
                    }
                }

                vec![(
                    RefNameBuf::from(bookmark_name),
                    ws.view().get_remote_bookmark(remote_ref_symbol),
                )]
            }
        };

        // check for conflicts
        let mut new_heads = vec![];
        for (_, bookmark_updates) in &mut remote_bookmark_updates {
            for (_, update) in bookmark_updates {
                if let Some(new_target) = &update.new_target {
                    new_heads.push(new_target.clone());
                }
            }
        }

        let mut old_heads = remote_bookmark_refs
            .into_iter()
            .flat_map(|(_, old_head)| old_head.target.added_ids())
            .cloned()
            .collect_vec();
        if old_heads.is_empty() {
            old_heads.push(ws.repo().store().root_commit_id().clone());
        }

        for commit in revset::walk_revs(ws.repo(), &new_heads, &old_heads)?
            .iter()
            .commits(ws.repo().store())
        {
            let commit = commit?;
            let mut reasons = vec![];
            if commit.description().is_empty() {
                reasons.push("it has no description");
            }
            if commit.author().name.is_empty()
                || commit.author().name == UserSettings::USER_NAME_PLACEHOLDER
                || commit.author().email.is_empty()
                || commit.author().email == UserSettings::USER_EMAIL_PLACEHOLDER
                || commit.committer().name.is_empty()
                || commit.committer().name == UserSettings::USER_NAME_PLACEHOLDER
                || commit.committer().email.is_empty()
                || commit.committer().email == UserSettings::USER_EMAIL_PLACEHOLDER
            {
                reasons.push("it has no author and/or committer set");
            }
            if commit.has_conflict() {
                reasons.push("it has conflicts");
            }
            if !reasons.is_empty() {
                precondition!(
                    "Won't push revision {} since {}",
                    ws.format_change_id(commit.id(), commit.change_id()).prefix,
                    reasons.join(" and ")
                );
            }
        }

        // check if there are any actual updates to push
        let has_updates = remote_bookmark_updates
            .iter()
            .any(|(_, updates)| !updates.is_empty());
        if !has_updates {
            match &self.refspec {
                GitRefspec::AllBookmarks { remote_name } => {
                    precondition!(
                        "No tracked bookmarks to push to remote '{remote_name}'. Track or push a bookmark first."
                    );
                }
                GitRefspec::AllRemotes { bookmark_ref } => {
                    let bookmark_name = bookmark_ref.as_bookmark()?;
                    precondition!("Bookmark '{bookmark_name}' is not tracked at any remote.");
                }
                GitRefspec::RemoteBookmark {
                    remote_name,
                    bookmark_ref,
                } => {
                    let bookmark_name = bookmark_ref.as_bookmark()?;
                    precondition!(
                        "Bookmark '{bookmark_name}' is not tracked at remote '{remote_name}'. Track it first."
                    );
                }
            }
        }

        // accumulate input requirements
        let mut auth_ctx = AuthContext::new(self.input);
        let event_sink = ws.sink();
        let subprocess_options = GitSubprocessOptions::from_settings(&ws.data.workspace_settings)?;

        // push to each remote
        for (remote_name, branch_updates) in remote_bookmark_updates.into_iter() {
            let targets = GitBranchPushTargets { branch_updates };

            let result = auth_ctx.with_callbacks(Some(event_sink.clone()), |cb, env| {
                let mut subprocess_options = subprocess_options.clone();
                subprocess_options.environment = env;

                git::push_branches(
                    tx.repo_mut(),
                    subprocess_options,
                    RemoteName::new(remote_name),
                    &targets,
                    cb,
                )
            });

            if let Err(err) = result {
                return Ok(auth_ctx.into_result(err.into()));
            }
        }

        match ws.finish_transaction(
            tx,
            match &self.refspec {
                GitRefspec::AllBookmarks { remote_name } => {
                    format!("push all tracked bookmarks to git remote {}", remote_name)
                }
                GitRefspec::AllRemotes { bookmark_ref } => {
                    format!(
                        "push {} to all tracked git remotes",
                        bookmark_ref.as_bookmark()?
                    )
                }
                GitRefspec::RemoteBookmark {
                    remote_name,
                    bookmark_ref,
                } => {
                    format!(
                        "push {} to git remote {}",
                        bookmark_ref.as_bookmark()?,
                        remote_name
                    )
                }
            },
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
impl Mutation for GitFetch {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let git_repo = match ws.git_repo() {
            Some(git_repo) => git_repo,
            None => precondition!("No git backend"),
        };

        let mut remote_patterns = Vec::new();
        match &self.refspec {
            GitRefspec::AllBookmarks { remote_name } => {
                remote_patterns.push((remote_name.clone(), None));
            }
            GitRefspec::AllRemotes { bookmark_ref } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                for remote_name in get_git_remote_names(&git_repo) {
                    remote_patterns.push((remote_name, Some(bookmark_name.to_owned())));
                }
            }
            GitRefspec::RemoteBookmark {
                remote_name,
                bookmark_ref,
            } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                remote_patterns.push((remote_name.clone(), Some(bookmark_name.to_owned())));
            }
        }

        // accumulate input requirements
        let mut auth_ctx = AuthContext::new(self.input);
        let progress_sender = ws.sink();
        let git_settings = GitSettings::from_settings(&ws.data.workspace_settings)?;
        let import_options = load_git_import_options(&git_settings, &ws.data.workspace_settings)?;

        for (remote_name, pattern) in &remote_patterns {
            let bookmark_expr = pattern
                .clone()
                .map(StringExpression::exact)
                .unwrap_or_else(StringExpression::all);
            let refspecs = git::expand_fetch_refspecs(RemoteName::new(remote_name), bookmark_expr)?;

            let result = auth_ctx.with_callbacks(Some(progress_sender.clone()), |cb, env| {
                let mut subprocess_options = git_settings.to_subprocess_options();
                subprocess_options.environment = env;

                let mut fetcher =
                    git::GitFetch::new(tx.repo_mut(), subprocess_options, &import_options)?;

                fetcher
                    .fetch(RemoteName::new(remote_name), refspecs, cb, None, None)
                    .context("failed to fetch")?;

                fetcher.import_refs().context("failed to import refs")?;

                Ok(())
            });

            if let Err(err) = result {
                return Ok(auth_ctx.into_result(err));
            }
        }

        match ws.finish_transaction(tx, "fetch from git remote(s)".to_string())? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

// this is another case where it would be nice if we could reuse jj-cli's error messages
#[async_trait(?Send)]
impl Mutation for UndoOperation {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let head_op = op_walk::resolve_op_with_repo(ws.repo(), "@")?; // XXX this should be behind an abstraction, maybe reused in snapshot
        let mut parent_ops = head_op.parents();

        let Some(parent_op) = parent_ops.next().transpose()? else {
            precondition!("Cannot undo repo initialization");
        };

        if parent_ops.next().is_some() {
            precondition!("Cannot undo a merge operation");
        };

        let mut tx = ws.start_transaction().await?;
        let repo_loader = tx.base_repo().loader();
        let head_repo = repo_loader.load_at(&head_op)?;
        let parent_repo = repo_loader.load_at(&parent_op)?;
        tx.repo_mut().merge(&head_repo, &parent_repo)?;
        let restored_view = tx.repo().view().store_view().clone();
        tx.repo_mut().set_view(restored_view);

        match ws.finish_transaction(tx, format!("undo operation {}", head_op.id().hex()))? {
            Some(new_status) => {
                let working_copy = ws.get_commit(ws.wc_id())?;
                let new_selection = Some(ws.format_header(&working_copy, None)?);
                Ok(MutationResult::Updated {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
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

fn combine_bookmarks(bookmark_names: &[impl Display]) -> String {
    match bookmark_names {
        [bookmark_name] => format!("bookmark {}", bookmark_name),
        bookmark_names => format!("bookmarks {}", bookmark_names.iter().join(", ")),
    }
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

fn classify_bookmark_push(
    bookmark_name: &str,
    remote_name: &str,
    targets: LocalAndRemoteRef,
) -> Result<Option<BookmarkPushUpdate>, String> {
    let push_action = refs::classify_bookmark_push_action(targets);
    match push_action {
        BookmarkPushAction::AlreadyMatches => Ok(None),
        BookmarkPushAction::Update(update) => Ok(Some(update)),
        BookmarkPushAction::LocalConflicted => {
            Err(format!("Bookmark {} is conflicted.", bookmark_name))
        }
        BookmarkPushAction::RemoteConflicted => Err(format!(
            "Bookmark {}@{} is conflicted. Try fetching first.",
            bookmark_name, remote_name
        )),
        BookmarkPushAction::RemoteUntracked => Err(format!(
            "Non-tracking remote bookmark {}@{} exists. Try tracking it first.",
            bookmark_name, remote_name
        )),
    }
}

async fn read_file_content(
    store: &Arc<Store>,
    tree: &MergedTree,
    path: &RepoPath,
) -> Result<Vec<u8>> {
    let entry = tree.path_value(path)?;
    match entry.into_resolved() {
        Ok(Some(TreeValue::File { id, .. })) => {
            let mut reader = store.read_file(path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            Ok(content)
        }
        Ok(Some(_)) => Ok(Vec::new()),
        Ok(None) => Ok(Vec::new()),
        Err(_) => {
            // handle conflicts by materializing them
            match conflicts::materialize_tree_value(
                store,
                path,
                tree.path_value(path)?,
                tree.labels(),
            )
            .await?
            {
                MaterializedTreeValue::FileConflict(file) => {
                    let mut content = Vec::new();
                    conflicts::materialize_merge_result(
                        &file.contents,
                        &file.labels,
                        &mut content,
                        &ConflictMaterializeOptions {
                            marker_style: ConflictMarkerStyle::Git,
                            marker_len: None,
                            merge: MergeOptions {
                                hunk_level: FileMergeHunkLevel::Line,
                                same_change: SameChange::Accept,
                            },
                        },
                    )?;
                    Ok(content)
                }
                _ => Ok(Vec::new()),
            }
        }
    }
}

/// Construct the sibling tree's file content by applying a hunk to its base.
///
/// The hunk was computed as a diff between `base` (the source commit's parent) and the
/// source commit. This function applies that diff to reconstruct the file content that
/// would exist in a virtual "sibling" commit containing only this hunk.
///
/// Line numbers must match exactly since the hunk was computed against this base.
#[allow(clippy::manual_strip)]
fn apply_hunk_to_base(base_content: &[u8], hunk: &crate::messages::ChangeHunk) -> Result<Vec<u8>> {
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
