use std::fmt::Display;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::backend::{CopyId, FileId, MergedTreeId, TreeValue};
use jj_lib::conflicts::{
    self, ConflictMarkerStyle, ConflictMaterializeOptions, MaterializedTreeValue,
};
use jj_lib::files::FileMergeHunkLevel;
use jj_lib::merge::{Merge, SameChange};
use jj_lib::merged_tree::{MergedTree, MergedTreeBuilder};
use jj_lib::ref_name::{RefNameBuf, RemoteName, RemoteNameBuf, RemoteRefSymbol};
use jj_lib::tree_merge::MergeOptions;
use jj_lib::{
    backend::{BackendError, CommitId},
    commit::Commit,
    git::{self, GitBranchPushTargets, REMOTE_NAME_FOR_LOCAL_GIT_REPO},
    matchers::{EverythingMatcher, FilesMatcher, Matcher},
    object_id::ObjectId as ObjectIdTrait,
    op_store::{RefTarget, RemoteRef, RemoteRefState},
    op_walk,
    refs::{self, BookmarkPushAction, BookmarkPushUpdate, LocalAndRemoteRef},
    repo::Repo,
    repo_path::RepoPath,
    revset::{self, RevsetIteratorExt},
    rewrite::{self, RebaseOptions, RebasedCommit},
    settings::UserSettings,
    store::Store,
    str_util::StringPattern,
};
use tokio::io::AsyncReadExt;

use crate::messages::{
    AbandonRevisions, BackoutRevisions, CheckoutRevision, CopyChanges, CopyHunk, CreateRef,
    CreateRevision, CreateRevisionBetween, DeleteRef, DescribeRevision, DuplicateRevisions,
    GitFetch, GitPush, InsertRevision, MoveChanges, MoveHunk, MoveRef, MoveRevision, MoveSource,
    MutationResult, RenameBranch, StoreRef, TrackBranch, TreePath, UndoOperation, UntrackBranch,
};

use super::Mutation;
use super::gui_util::{WorkspaceSession, get_git_remote_names};

macro_rules! precondition {
    ($($args:tt)*) => {
        return Ok(MutationResult::PreconditionError { message: format!($($args)*) })
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for AbandonRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let abandoned_ids = self
            .ids
            .into_iter()
            .map(|id| CommitId::try_from_hex(&id.hex).expect("frontend-validated id"))
            .collect_vec();

        if ws.check_immutable(abandoned_ids.clone())? {
            precondition!("Some revisions are immutable");
        }

        for id in &abandoned_ids {
            let commit = tx
                .repo()
                .store()
                .get_commit(id)
                .context("Failed to lookup commit")?;
            tx.repo_mut().record_abandoned_commit(&commit);
        }
        tx.repo_mut().rebase_descendants()?;

        let transaction_description = if abandoned_ids.len() == 1 {
            format!("abandon commit {}", abandoned_ids[0].hex())
        } else {
            format!(
                "abandon commit {} and {} more",
                abandoned_ids[0].hex(),
                abandoned_ids.len() - 1
            )
        };

        match ws.finish_transaction(tx, transaction_description)? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for BackoutRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        if self.ids.len() != 1 {
            precondition!("Not implemented for >1 rev");
        }

        let mut tx = ws.start_transaction()?;

        let working_copy = ws.get_commit(ws.wc_id())?;
        let reverted = ws.resolve_multiple_changes(self.ids)?;
        let reverted_parents: Result<Vec<_>, BackendError> = reverted[0].parents().collect();

        let old_base_tree = rewrite::merge_commit_trees(tx.repo(), &reverted_parents?).await?;
        let new_base_tree = working_copy.tree()?;
        let old_tree = reverted[0].tree()?;
        let new_tree = new_base_tree.merge(old_tree, old_base_tree).await?;

        tx.repo_mut()
            .rewrite_commit(&working_copy)
            .set_tree_id(new_tree.id())
            .write()?;

        match ws.finish_transaction(tx, format!("back out commit {}", reverted[0].id().hex()))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for CheckoutRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let edited = ws.resolve_single_change(&self.id)?;

        if ws.check_immutable(vec![edited.id().clone()])? {
            precondition!("Revision is immutable");
        }

        if edited.id() == ws.wc_id() {
            return Ok(MutationResult::Unchanged);
        }

        tx.repo_mut().edit(ws.name().to_owned(), &edited)?;

        match ws.finish_transaction(tx, format!("edit commit {}", edited.id().hex()))? {
            Some(new_status) => {
                let new_selection = ws.format_header(&edited, Some(false))?;
                Ok(MutationResult::UpdatedSelection {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for CreateRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let parents_revset = ws.evaluate_revset_changes(
            &self
                .parent_ids
                .into_iter()
                .map(|id| id.change)
                .collect_vec(),
        )?;

        let parent_ids: Result<_, _> = parents_revset.iter().collect();
        let parent_commits = ws.resolve_multiple(parents_revset)?;
        let merged_tree = rewrite::merge_commit_trees(tx.repo(), &parent_commits).await?;

        let new_commit = tx
            .repo_mut()
            .new_commit(parent_ids?, merged_tree.id())
            .write()?;

        tx.repo_mut().edit(ws.name().to_owned(), &new_commit)?;

        match ws.finish_transaction(tx, "new empty commit")? {
            Some(new_status) => {
                let new_selection = ws.format_header(&new_commit, Some(false))?;
                Ok(MutationResult::UpdatedSelection {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for CreateRevisionBetween {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        eprintln!("CreateREvisionBetween execute()");
        let mut tx = ws.start_transaction()?;

        let parent_id = ws
            .resolve_single_commit(&self.after_id)
            .context("resolve after_id")?;
        let parent_ids = vec![parent_id.id().clone()];
        let parent_commits = vec![parent_id];
        let merged_tree = rewrite::merge_commit_trees(tx.repo(), &parent_commits).await?;

        let new_commit = tx
            .repo_mut()
            .new_commit(parent_ids, merged_tree.id())
            .write()?;

        let before_commit = ws
            .resolve_single_change(&self.before_id)
            .context("resolve before_id")?;
        if ws.check_immutable(vec![before_commit.id().clone()])? {
            precondition!("'Before' revision is immutable");
        }

        rewrite::rebase_commit(tx.repo_mut(), before_commit, vec![new_commit.id().clone()]).await?;

        tx.repo_mut().edit(ws.name().to_owned(), &new_commit)?;

        match ws.finish_transaction(tx, "new empty commit")? {
            Some(new_status) => {
                let new_selection = ws.format_header(&new_commit, Some(false))?;
                Ok(MutationResult::UpdatedSelection {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for DescribeRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let described = ws.resolve_single_change(&self.id)?;

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
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for DuplicateRevisions {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let clonees = ws.resolve_multiple_changes(self.ids)?; // in reverse topological order
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
                    let new_selection = ws.format_header(new_commit, None)?;
                    Ok(MutationResult::UpdatedSelection {
                        new_status,
                        new_selection,
                    })
                } else {
                    Ok(MutationResult::Updated { new_status })
                }
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for InsertRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let target = ws
            .resolve_single_change(&self.id)
            .context("resolve change_id")?;
        let before = ws
            .resolve_single_change(&self.before_id)
            .context("resolve before_id")?;
        let after = ws
            .resolve_single_change(&self.after_id)
            .context("resolve after_id")?;

        if ws.check_immutable(vec![target.id().clone(), before.id().clone()])? {
            precondition!("Some revisions are immutable");
        }

        // rebase the target's children
        let rebased_children = ws.disinherit_children(&mut tx, &target)?;

        // update after, which may have been a descendant of target
        let after_id = rebased_children
            .get(after.id())
            .unwrap_or(after.id())
            .clone();

        // rebase the target (which now has no children), then the new post-target tree atop it
        let rebased_id = target.id().hex();
        let target = rewrite::rebase_commit(tx.repo_mut(), target, vec![after_id]).await?;
        rewrite::rebase_commit(tx.repo_mut(), before, vec![target.id().clone()]).await?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for MoveRevision {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let target = ws.resolve_single_change(&self.id)?;
        let parents = ws.resolve_multiple_changes(self.parent_ids)?;

        if ws.check_immutable(vec![target.id().clone()])? {
            precondition!("Revision {} is immutable", self.id.change.prefix);
        }

        // rebase the target's children
        let rebased_children = ws.disinherit_children(&mut tx, &target)?;

        // update parents, which may have been descendants of the target
        let parent_ids: Vec<_> = parents
            .iter()
            .map(|new_parent| {
                rebased_children
                    .get(new_parent.id())
                    .unwrap_or(new_parent.id())
                    .clone()
            })
            .collect();

        // rebase the target itself
        let rebased_id = target.id().hex();
        rewrite::rebase_commit(tx.repo_mut(), target, parent_ids).await?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for MoveSource {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let target = ws.resolve_single_change(&self.id)?;
        let parent_ids = ws
            .resolve_multiple_commits(&self.parent_ids)?
            .into_iter()
            .map(|commit| commit.id().clone())
            .collect();

        if ws.check_immutable(vec![target.id().clone()])? {
            precondition!("Revision {} is immutable", self.id.change.prefix);
        }

        // just rebase the target, which will also rebase its descendants
        let rebased_id = target.id().hex();
        rewrite::rebase_commit(tx.repo_mut(), target, parent_ids).await?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for MoveChanges {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from = ws.resolve_single_change(&self.from_id)?;
        let mut to = ws.resolve_single_commit(&self.to_id)?;
        let matcher = build_matcher(&self.paths)?;

        if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
            precondition!("Revisions are immutable");
        }

        // construct a split tree and a remainder tree by copying changes from child to parent and from parent to child
        let from_tree = from.tree()?;
        let from_parents: Result<Vec<_>, _> = from.parents().collect();
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &from_parents?).await?;
        let split_tree_id =
            rewrite::restore_tree(&from_tree, &parent_tree, matcher.as_ref()).await?;
        let split_tree = tx.repo().store().get_root_tree(&split_tree_id)?;
        let remainder_tree_id =
            rewrite::restore_tree(&parent_tree, &from_tree, matcher.as_ref()).await?;
        let remainder_tree = tx.repo().store().get_root_tree(&remainder_tree_id)?;

        // abandon or rewrite source
        let abandon_source = remainder_tree.id() == parent_tree.id();
        if abandon_source {
            tx.repo_mut().record_abandoned_commit(&from);
        } else {
            tx.repo_mut()
                .rewrite_commit(&from)
                .set_tree_id(remainder_tree.id().clone())
                .write()?;
        }

        // rebase descendants of source, which may include destination
        if tx.repo().index().is_ancestor(from.id(), to.id())? {
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
        let to_tree = to.tree()?;
        let new_to_tree = to_tree
            .merge(parent_tree.clone(), split_tree.clone())
            .await?;
        let description = combine_messages(&from, &to, abandon_source);
        tx.repo_mut()
            .rewrite_commit(&to)
            .set_tree_id(new_to_tree.id().clone())
            .set_description(description)
            .write()?;

        match ws.finish_transaction(
            tx,
            format!("move changes from {} to {}", from.id().hex(), to.id().hex()),
        )? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for CopyChanges {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from_tree = ws.resolve_single_commit(&self.from_id)?.tree()?;
        let to = ws.resolve_single_change(&self.to_id)?;
        let matcher = build_matcher(&self.paths)?;

        if ws.check_immutable(vec![to.id().clone()])? {
            precondition!("Revisions are immutable");
        }

        // construct a restore tree - the destination with some portions overwritten by the source
        let to_tree = to.tree()?;
        let new_to_tree_id = rewrite::restore_tree(&from_tree, &to_tree, matcher.as_ref()).await?;
        if &new_to_tree_id == to.tree_id() {
            Ok(MutationResult::Unchanged)
        } else {
            tx.repo_mut()
                .rewrite_commit(&to)
                .set_tree_id(new_to_tree_id)
                .write()?;

            tx.repo_mut().rebase_descendants()?;

            match ws.finish_transaction(tx, format!("restore into commit {}", to.id().hex()))? {
                Some(new_status) => Ok(MutationResult::Updated { new_status }),
                None => Ok(MutationResult::Unchanged),
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for TrackBranch {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        match self.r#ref {
            StoreRef::Tag { tag_name } => {
                precondition!("{} is a tag and cannot be tracked", tag_name);
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                precondition!("{} is a local bookmark and cannot be tracked", branch_name);
            }
            StoreRef::RemoteBookmark {
                branch_name,
                remote_name,
                ..
            } => {
                let mut tx = ws.start_transaction()?;
                let branch_name_ref = RefNameBuf::from(branch_name);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &branch_name_ref,
                    remote: &remote_name_ref,
                };

                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_bookmark(remote_ref_symbol);

                if remote_ref.is_tracked() {
                    precondition!(
                        "{:?}@{:?} is already tracked",
                        branch_name_ref.as_str(),
                        remote_name_ref.as_str()
                    );
                }

                tx.repo_mut().track_remote_bookmark(remote_ref_symbol)?;

                match ws.finish_transaction(
                    tx,
                    format!(
                        "track remote bookmark {:?}@{:?}",
                        branch_name_ref.as_str(),
                        remote_name_ref.as_str()
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for UntrackBranch {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let mut untracked = Vec::new();
        match self.r#ref {
            StoreRef::Tag { tag_name } => {
                precondition!("{} is a tag and cannot be untracked", tag_name);
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                // untrack all remotes
                for (remote_ref_symbol, remote_ref) in ws.view().remote_bookmarks_matching(
                    &StringPattern::exact(branch_name).to_matcher(),
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
                branch_name,
                remote_name,
                ..
            } => {
                let branch_name_ref = RefNameBuf::from(branch_name);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &branch_name_ref,
                    remote: &remote_name_ref,
                };
                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_bookmark(remote_ref_symbol);

                if !remote_ref.is_tracked() {
                    precondition!(
                        "{:?}@{:?} is not tracked",
                        branch_name_ref.as_str(),
                        remote_name_ref.as_str()
                    );
                }

                tx.repo_mut().untrack_remote_bookmark(remote_ref_symbol);
                untracked.push(format!(
                    "{}@{}",
                    branch_name_ref.as_str(),
                    remote_name_ref.as_str()
                ));
            }
        }

        match ws.finish_transaction(
            tx,
            format!("untrack remote {}", combine_bookmarks(&untracked)),
        )? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for RenameBranch {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let old_name = self.r#ref.as_branch()?;
        let old_name_ref = RefNameBuf::from(old_name);

        let ref_target = ws.view().get_local_bookmark(&old_name_ref).clone();
        if ref_target.is_absent() {
            precondition!("No such bookmark: {}", old_name_ref.as_str());
        }

        let new_name_ref = RefNameBuf::from(self.new_name);
        if ws.view().get_local_bookmark(&new_name_ref).is_present() {
            precondition!("Bookmark already exists: {}", new_name_ref.as_str());
        }

        let mut tx = ws.start_transaction()?;

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
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for CreateRef {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let commit = ws.resolve_single_change(&self.id)?;

        match self.r#ref {
            StoreRef::RemoteBookmark {
                branch_name,
                remote_name,
                ..
            } => {
                precondition!(
                    "{}@{} is a remote bookmark and cannot be created",
                    branch_name,
                    remote_name
                );
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                let branch_name_ref = RefNameBuf::from(branch_name);
                let existing_branch = ws.view().get_local_bookmark(&branch_name_ref);
                if existing_branch.is_present() {
                    precondition!("{} already exists", branch_name_ref.as_str());
                }

                tx.repo_mut().set_local_bookmark_target(
                    &branch_name_ref,
                    RefTarget::normal(commit.id().clone()),
                );

                match ws.finish_transaction(
                    tx,
                    format!(
                        "create {} pointing to commit {}",
                        branch_name_ref.as_str(),
                        ws.format_commit_id(commit.id()).hex
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
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
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for DeleteRef {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        match self.r#ref {
            StoreRef::RemoteBookmark {
                branch_name,
                remote_name,
                ..
            } => {
                let mut tx = ws.start_transaction()?;

                // forget the bookmark entirely - when target is absent, it's removed from the view
                let remote_ref = RemoteRef {
                    target: RefTarget::absent(),
                    state: RemoteRefState::New,
                };
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let branch_name_ref = RefNameBuf::from(branch_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &branch_name_ref,
                    remote: &remote_name_ref,
                };

                tx.repo_mut()
                    .set_remote_bookmark(remote_ref_symbol, remote_ref);

                match ws.finish_transaction(
                    tx,
                    format!(
                        "forget {}@{}",
                        branch_name_ref.as_str(),
                        remote_name_ref.as_str()
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                let branch_name_ref = RefNameBuf::from(branch_name);
                let mut tx = ws.start_transaction()?;

                tx.repo_mut()
                    .set_local_bookmark_target(&branch_name_ref, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget {}", branch_name_ref.as_str()))? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name } => {
                let tag_name_ref = RefNameBuf::from(tag_name);
                let mut tx = ws.start_transaction()?;

                tx.repo_mut()
                    .set_local_tag_target(&tag_name_ref, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget tag {}", tag_name_ref.as_str()))? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

// does not currently enforce fast-forwards
#[async_trait::async_trait(?Send)]
impl Mutation for MoveRef {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let commit = ws.resolve_single_change(&self.to_id)?;

        match self.r#ref {
            StoreRef::RemoteBookmark {
                branch_name,
                remote_name,
                ..
            } => {
                precondition!("Bookmark is remote: {branch_name}@{remote_name}")
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                let branch_name_ref = RefNameBuf::from(branch_name);
                let old_target = ws.view().get_local_bookmark(&branch_name_ref);
                if old_target.is_absent() {
                    precondition!("No such bookmark: {:?}", branch_name_ref.as_str());
                }

                tx.repo_mut().set_local_bookmark_target(
                    &branch_name_ref,
                    RefTarget::normal(commit.id().clone()),
                );

                match ws.finish_transaction(
                    tx,
                    format!(
                        "point {:?} to commit {}",
                        &branch_name_ref,
                        commit.id().hex()
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
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
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for MoveHunk {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from = ws.resolve_single_change(&self.from_id)?;
        let to = ws.resolve_single_commit(&self.to_id)?;
        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;

        let from_id = from.id().clone();
        let to_id = to.id().clone();

        if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
            precondition!("Revisions are immutable");
        }

        // get parent tree from which to calculate split and remainder trees
        let from_tree = from.tree()?;
        let from_parents: Result<Vec<_>, _> = from.parents().collect();
        let from_parents = from_parents?;
        if from_parents.len() != 1 {
            precondition!("Cannot move hunk from a merge commit");
        }
        let parent_tree = from_parents[0].tree()?;

        // construct hunk_tree: parent_tree with the hunk applied
        let store = tx.repo().store();
        let parent_content = read_file_content(store, &parent_tree, &repo_path).await?;
        let hunk_content = apply_hunk(&parent_content, &self.hunk)?;
        let hunk_blob_id = store
            .write_file(&repo_path, &mut hunk_content.as_slice())
            .await?;
        let hunk_executable = match from_tree.path_value(&repo_path)?.into_resolved() {
            Ok(Some(TreeValue::File { executable, .. })) => executable,
            Ok(_) => false,
            Err(_) => false,
        };
        let hunk_tree_id = update_tree_entry(
            store,
            &parent_tree,
            &repo_path,
            hunk_blob_id,
            hunk_executable,
        )?;
        let hunk_tree = store.get_root_tree(&hunk_tree_id)?;

        // check if commits are related, which disallows conflict-free copies
        let from_is_ancestor = tx.repo().index().is_ancestor(from.id(), to.id())?;
        let to_is_ancestor = tx.repo().index().is_ancestor(to.id(), from.id())?;
        let is_related = from_is_ancestor || to_is_ancestor;

        let (remainder_tree, new_to_tree) = if is_related {
            // for related commits, we need 3-way merge (may create conflicts)
            let remainder = from_tree
                .clone()
                .merge(hunk_tree.clone(), parent_tree.clone())
                .await?;
            let to_tree = to.tree()?;
            let new_to = to_tree
                .merge(parent_tree.clone(), hunk_tree.clone())
                .await?;
            (remainder, new_to)
        } else {
            // for unrelated commits, apply hunk directly to avoid conflicts
            let to_tree = to.tree()?;
            let to_content = read_file_content(store, &to_tree, &repo_path).await?;
            let new_to_content = apply_hunk(&to_content, &self.hunk)?;
            let new_to_blob_id = store
                .write_file(&repo_path, &mut new_to_content.as_slice())
                .await?;
            let new_to_tree_id =
                update_tree_entry(store, &to_tree, &repo_path, new_to_blob_id, hunk_executable)?;
            let new_to = store.get_root_tree(&new_to_tree_id)?;

            // remove hunk from source with reverse hunk
            let reverse_hunk = crate::messages::ChangeHunk {
                location: self.hunk.location.clone(),
                lines: crate::messages::MultilineString {
                    lines: self
                        .hunk
                        .lines
                        .lines
                        .iter()
                        .filter_map(|line| {
                            if line.starts_with('+') {
                                Some(format!("-{}", &line[1..]))
                            } else if line.starts_with('-') {
                                Some(format!("+{}", &line[1..]))
                            } else {
                                Some(line.clone())
                            }
                        })
                        .collect(),
                },
            };
            let from_content = read_file_content(store, &from_tree, &repo_path).await?;
            let remainder_content = apply_hunk(&from_content, &reverse_hunk)?;
            let remainder_blob_id = store
                .write_file(&repo_path, &mut remainder_content.as_slice())
                .await?;
            let remainder_tree_id = update_tree_entry(
                store,
                &from_tree,
                &repo_path,
                remainder_blob_id,
                hunk_executable,
            )?;
            let remainder = store.get_root_tree(&remainder_tree_id)?;

            (remainder, new_to)
        };

        // abandon or rewrite source and destination based on ancestry
        let abandon_source = remainder_tree.id() == parent_tree.id();

        if abandon_source {
            // simple case: no source left
            if to_is_ancestor {
                precondition!(
                    "Moving a hunk from a commit that becomes empty to an ancestor is not supported"
                );
            }
            tx.repo_mut().record_abandoned_commit(&from);

            // apply hunk to destination
            let description = combine_messages(&from, &to, abandon_source);
            tx.repo_mut()
                .rewrite_commit(&to)
                .set_tree_id(new_to_tree.id().clone())
                .set_description(description)
                .write()?;
        } else if to_is_ancestor {
            // special case: descendant-to-ancestor

            let description = combine_messages(&from, &to, abandon_source);
            let new_to = tx
                .repo_mut()
                .rewrite_commit(&to)
                .set_tree_id(new_to_tree.id().clone())
                .set_description(description)
                .write()?;

            // recompute the source's tree after the destination has the hunk applied
            let child_new_tree = new_to_tree.clone().merge(parent_tree, from_tree).await?;

            // rebase source
            tx.repo_mut()
                .rewrite_commit(&from)
                .set_parents(vec![new_to.id().clone()])
                .set_tree_id(child_new_tree.id().clone())
                .write()?;
        } else {
            // general case: unrelated or ancestor-to-descendant
            tx.repo_mut()
                .rewrite_commit(&from)
                .set_tree_id(remainder_tree.id().clone())
                .write()?;

            let description = combine_messages(&from, &to, abandon_source);
            let mut to = to;

            // if destination is a descendant, rebase it after modifying source
            if from_is_ancestor {
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

            tx.repo_mut()
                .rewrite_commit(&to)
                .set_tree_id(new_to_tree.id().clone())
                .set_description(description)
                .write()?;

            tx.repo_mut().rebase_descendants()?;
        }

        match ws.finish_transaction(
            tx,
            format!(
                "move hunk in {} from {} to {}",
                self.path.repo_path,
                from_id.hex(),
                to_id.hex()
            ),
        )? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for CopyHunk {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from = ws.resolve_single_commit(&self.from_id)?;
        let to = ws.resolve_single_change(&self.to_id)?;
        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;

        if ws.check_immutable(vec![to.id().clone()])? {
            precondition!("Revision is immutable");
        }

        let store = tx.repo().store();
        let to_tree = to.tree()?;

        // vheck for conflicts in destination
        let to_path_value = to_tree.path_value(&repo_path)?;
        if to_path_value.into_resolved().is_err() {
            precondition!("Cannot restore hunk: destination file has conflicts");
        }

        // read destination content
        let to_content = read_file_content(store, &to_tree, &repo_path).await?;
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
        let from_tree = from.tree()?;
        let from_content = read_file_content(store, &from_tree, &repo_path).await?;
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
            .write_file(&repo_path, &mut new_to_content.as_slice())
            .await?;

        let to_executable = match to_tree.path_value(&repo_path)?.into_resolved() {
            Ok(Some(TreeValue::File { executable, .. })) => executable,
            _ => false,
        };

        let new_to_tree_id =
            update_tree_entry(store, &to_tree, &repo_path, new_to_blob_id, to_executable)?;

        // rewrite destination
        tx.repo_mut()
            .rewrite_commit(&to)
            .set_tree_id(new_to_tree_id)
            .write()?;

        tx.repo_mut().rebase_descendants()?;

        match ws.finish_transaction(
            tx,
            format!(
                "restore hunk in {} from {} into {}",
                self.path.repo_path, self.from_id.hex, self.to_id.commit.hex
            ),
        )? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for GitPush {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        // determine bookmarks to push, recording the old and new commits
        let mut remote_branch_updates: Vec<(&str, Vec<(RefNameBuf, refs::BookmarkPushUpdate)>)> =
            Vec::new();
        let remote_branch_refs: Vec<_> = match &*self {
            GitPush::AllBookmarks { remote_name } => {
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let mut branch_updates = Vec::new();
                for (branch_name, targets) in ws.view().local_remote_bookmarks(&remote_name_ref) {
                    if !targets.remote_ref.is_tracked() {
                        continue;
                    }

                    match classify_branch_push(branch_name.as_str(), remote_name, targets) {
                        Err(message) => return Ok(MutationResult::PreconditionError { message }),
                        Ok(None) => (),
                        Ok(Some(update)) => branch_updates.push((branch_name.to_owned(), update)),
                    }
                }
                remote_branch_updates.push((remote_name, branch_updates));

                ws.view()
                    .remote_bookmarks(&remote_name_ref)
                    .map(|(name, remote_ref)| (name.to_owned(), remote_ref))
                    .collect()
            }
            GitPush::AllRemotes { branch_ref } => {
                let branch_name = branch_ref.as_branch()?;
                let branch_name_ref = RefNameBuf::from(branch_name);

                let mut remote_branch_refs = Vec::new();
                for (remote_name, group) in ws
                    .view()
                    .all_remote_bookmarks()
                    .filter_map(|(remote_ref_symbol, remote_ref)| {
                        if remote_ref.is_tracked()
                            && remote_ref_symbol.name == branch_name_ref
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
                    let mut branch_updates = Vec::new();
                    for (_, remote_ref) in group {
                        let targets = LocalAndRemoteRef {
                            local_target: ws.view().get_local_bookmark(&branch_name_ref),
                            remote_ref,
                        };
                        match classify_branch_push(branch_name, remote_name.as_str(), targets) {
                            Err(message) => {
                                return Ok(MutationResult::PreconditionError { message });
                            }
                            Ok(None) => (),
                            Ok(Some(update)) => {
                                branch_updates.push((RefNameBuf::from(branch_name), update))
                            }
                        }
                        remote_branch_refs.push((RefNameBuf::from(branch_name), remote_ref));
                    }
                    remote_branch_updates.push((remote_name.as_str(), branch_updates));
                }

                remote_branch_refs
            }
            GitPush::RemoteBookmark {
                remote_name,
                branch_ref,
            } => {
                let branch_name = branch_ref.as_branch()?;
                let branch_name_ref = RefNameBuf::from(branch_name);
                let local_target = ws.view().get_local_bookmark(&branch_name_ref);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &branch_name_ref,
                    remote: &remote_name_ref,
                };
                let remote_ref = ws.view().get_remote_bookmark(remote_ref_symbol);

                match classify_branch_push(
                    branch_name,
                    remote_name,
                    LocalAndRemoteRef {
                        local_target,
                        remote_ref,
                    },
                ) {
                    Err(message) => return Ok(MutationResult::PreconditionError { message }),
                    Ok(None) => (),
                    Ok(Some(update)) => {
                        remote_branch_updates
                            .push((remote_name, vec![(RefNameBuf::from(branch_name), update)]));
                    }
                }

                vec![(
                    RefNameBuf::from(branch_name),
                    ws.view().get_remote_bookmark(remote_ref_symbol),
                )]
            }
        };

        // check for conflicts
        let mut new_heads = vec![];
        for (_, branch_updates) in &mut remote_branch_updates {
            for (_, update) in branch_updates {
                if let Some(new_target) = &update.new_target {
                    new_heads.push(new_target.clone());
                }
            }
        }

        let mut old_heads = remote_branch_refs
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
                    ws.format_change_id(commit.change_id()).prefix,
                    reasons.join(" and ")
                );
            }
        }

        // push to each remote
        for (remote_name, branch_updates) in remote_branch_updates.into_iter() {
            let targets = GitBranchPushTargets { branch_updates };
            let git_settings = ws.data.workspace_settings.git_settings()?;

            ws.session.callbacks.with_git(tx.repo_mut(), &|repo, cb| {
                git::push_branches(
                    repo,
                    &git_settings,
                    RemoteName::new(remote_name),
                    &targets,
                    cb,
                )?;
                Ok(())
            })?;
        }

        match ws.finish_transaction(
            tx,
            match *self {
                GitPush::AllBookmarks { remote_name } => {
                    format!("push all tracked branches to git remote {}", remote_name)
                }
                GitPush::AllRemotes { branch_ref } => {
                    format!(
                        "push {} to all tracked git remotes",
                        branch_ref.as_branch()?
                    )
                }
                GitPush::RemoteBookmark {
                    remote_name,
                    branch_ref,
                } => {
                    format!(
                        "push {} to git remote {}",
                        branch_ref.as_branch()?,
                        remote_name
                    )
                }
            },
        )? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Mutation for GitFetch {
    async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let git_repo = match ws.git_repo() {
            Some(git_repo) => git_repo,
            None => precondition!("No git backend"),
        };

        let mut remote_patterns = Vec::new();
        match *self {
            GitFetch::AllBookmarks { remote_name } => {
                remote_patterns.push((remote_name, None));
            }
            GitFetch::AllRemotes { branch_ref } => {
                let branch_name = branch_ref.as_branch()?;
                for remote_name in get_git_remote_names(&git_repo) {
                    remote_patterns.push((remote_name, Some(branch_name.to_owned())));
                }
            }
            GitFetch::RemoteBookmark {
                remote_name,
                branch_ref,
            } => {
                let branch_name = branch_ref.as_branch()?;
                remote_patterns.push((remote_name, Some(branch_name.to_owned())));
            }
        }
        let git_settings = ws.data.workspace_settings.git_settings()?;

        for (remote_name, pattern) in remote_patterns {
            ws.session.callbacks.with_git(tx.repo_mut(), &|repo, cb| {
                let mut fetcher = git::GitFetch::new(repo, &git_settings)?;
                let refspecs = git::expand_fetch_refspecs(
                    &RemoteName::new(&remote_name),
                    vec![
                        pattern
                            .clone()
                            .map(StringPattern::exact)
                            .unwrap_or_else(StringPattern::all),
                    ],
                )?;
                fetcher
                    .fetch(RemoteName::new(&remote_name), refspecs, cb, None, None)
                    .context("failed to fetch")?;
                fetcher.import_refs().context("failed to import refs")?;
                Ok(())
            })?;
        }

        match ws.finish_transaction(tx, "fetch from git remote(s)".to_string())? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

// this is another case where it would be nice if we could reuse jj-cli's error messages
#[async_trait::async_trait(?Send)]
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

        let mut tx = ws.start_transaction()?;
        let repo_loader = tx.base_repo().loader();
        let head_repo = repo_loader.load_at(&head_op)?;
        let parent_repo = repo_loader.load_at(&parent_op)?;
        tx.repo_mut().merge(&head_repo, &parent_repo)?;
        let restored_view = tx.repo().view().store_view().clone();
        tx.repo_mut().set_view(restored_view);

        match ws.finish_transaction(tx, format!("undo operation {}", head_op.id().hex()))? {
            Some(new_status) => {
                let working_copy = ws.get_commit(ws.wc_id())?;
                let new_selection = ws.format_header(&working_copy, None)?;
                Ok(MutationResult::UpdatedSelection {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

fn combine_messages(source: &Commit, destination: &Commit, abandon_source: bool) -> String {
    if abandon_source {
        if source.description().is_empty() {
            destination.description().to_owned()
        } else if destination.description().is_empty() {
            source.description().to_owned()
        } else {
            destination.description().to_owned() + "\n" + source.description()
        }
    } else {
        destination.description().to_owned()
    }
}

fn combine_bookmarks(branch_names: &[impl Display]) -> String {
    match branch_names {
        [branch_name] => format!("bookmark {}", branch_name),
        branch_names => format!("bookmarks {}", branch_names.iter().join(", ")),
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

fn classify_branch_push(
    branch_name: &str,
    remote_name: &str,
    targets: LocalAndRemoteRef,
) -> Result<Option<BookmarkPushUpdate>, String> {
    let push_action = refs::classify_bookmark_push_action(targets);
    match push_action {
        BookmarkPushAction::AlreadyMatches => Ok(None),
        BookmarkPushAction::Update(update) => Ok(Some(update)),
        BookmarkPushAction::LocalConflicted => {
            Err(format!("Bookmark {} is conflicted.", branch_name))
        }
        BookmarkPushAction::RemoteConflicted => Err(format!(
            "Bookmark {}@{} is conflicted. Try fetching first.",
            branch_name, remote_name
        )),
        BookmarkPushAction::RemoteUntracked => Err(format!(
            "Non-tracking remote bookmark {}@{} exists. Try tracking it first.",
            branch_name, remote_name
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
            match conflicts::materialize_tree_value(store, path, tree.path_value(path)?).await? {
                MaterializedTreeValue::FileConflict(file) => {
                    let mut content = Vec::new();
                    conflicts::materialize_merge_result(
                        &file.contents,
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

// imperfect, but will work for many real-world moves
fn find_hunk_position(
    base_lines: &[&str],
    hunk: &crate::messages::ChangeHunk,
    suggested_start: usize,
) -> Result<usize> {
    let context_lines: Vec<&str> = hunk
        .lines
        .lines
        .iter()
        .filter(|line| line.starts_with(' ') || line.starts_with('-'))
        .map(|line| line[1..].trim_end())
        .collect();

    if context_lines.is_empty() {
        return Ok(suggested_start.min(base_lines.len()));
    }

    if base_lines.len() < context_lines.len() {
        return Err(anyhow!(
            "File has {} lines but hunk requires at least {} lines of context",
            base_lines.len(),
            context_lines.len()
        ));
    }

    for start_idx in 0..=(base_lines.len() - context_lines.len()) {
        let matches = context_lines
            .iter()
            .enumerate()
            .all(|(i, &context_line)| base_lines[start_idx + i].trim_end() == context_line);

        if matches {
            return Ok(start_idx);
        }
    }

    Err(anyhow!("Couldn't find a good  location to apply the hunk."))
}

// XXX does not use 3-way merge, which reduces conflicts but is imprecise
fn apply_hunk(content: &[u8], hunk: &crate::messages::ChangeHunk) -> Result<Vec<u8>> {
    let base_text = String::from_utf8_lossy(content);
    let base_lines: Vec<&str> = base_text.lines().collect();
    let ends_with_newline = content.ends_with(b"\n");

    let mut result_lines: Vec<String> = Vec::new();
    let mut base_line_idx: usize;
    let mut hunk_lines = hunk.lines.lines.iter().peekable();

    let hunk_start_line_0_based = hunk.location.from_file.start.saturating_sub(1);
    let actual_start = find_hunk_position(&base_lines, hunk, hunk_start_line_0_based)?;

    result_lines.extend(base_lines[..actual_start].iter().map(|s| s.to_string()));
    base_line_idx = actual_start;

    while let Some(hunk_line) = hunk_lines.next() {
        if hunk_line.starts_with(' ') || hunk_line.starts_with('-') {
            let hunk_content_part = &hunk_line[1..];
            if base_line_idx < base_lines.len()
                && base_lines[base_line_idx].trim_end() == hunk_content_part.trim_end()
            {
                if hunk_line.starts_with(' ') {
                    result_lines.push(base_lines[base_line_idx].to_string());
                }
                base_line_idx += 1;
            } else {
                return Err(anyhow!(
                    "Hunk mismatch at line {}: expected '{}', found '{}'",
                    base_line_idx,
                    hunk_content_part.trim_end(),
                    base_lines
                        .get(base_line_idx)
                        .map_or("<EOF>", |l| l.trim_end())
                ));
            }
        } else if hunk_line.starts_with('+') {
            let added_content = hunk_line[1..].trim_end_matches('\n');
            result_lines.push(added_content.to_string());
        } else {
            return Err(anyhow!("Malformed hunk line: {}", hunk_line));
        }
    }
    result_lines.extend(base_lines[base_line_idx..].iter().map(|s| s.to_string()));

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
    store: &Arc<jj_lib::store::Store>,
    original_tree: &MergedTree,
    path: &RepoPath,
    new_blob: FileId,
    executable: bool,
) -> Result<MergedTreeId, anyhow::Error> {
    let mut builder = MergedTreeBuilder::new(original_tree.id().clone());
    builder.set_or_remove(
        path.to_owned(),
        Merge::normal(TreeValue::File {
            id: new_blob,
            executable,
            copy_id: CopyId::placeholder(),
        }),
    );
    let new_tree_id = builder.write_tree(store)?;
    Ok(new_tree_id)
}
