use std::fmt::Display;

use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    backend::{BackendError, CommitId},
    commit::Commit,
    git::{self, GitBranchPushTargets, REMOTE_NAME_FOR_LOCAL_GIT_REPO},
    matchers::{EverythingMatcher, FilesMatcher, Matcher},
    object_id::ObjectId,
    op_store::{RefTarget, RemoteRef, RemoteRefState},
    op_walk,
    refs::{self, BookmarkPushAction, BookmarkPushUpdate, LocalAndRemoteRef},
    repo::Repo,
    repo_path::RepoPath,
    revset::{self, RevsetIteratorExt},
    rewrite::{self, RebaseOptions, RebasedCommit},
    settings::UserSettings,
    str_util::StringPattern,
};

use super::{gui_util::WorkspaceSession, Mutation};
use crate::messages::{
    AbandonRevisions, BackoutRevisions, CheckoutRevision, CopyChanges, CreateRef, CreateRevision,
    DeleteRef, DescribeRevision, DuplicateRevisions, GitFetch, GitPush, InsertRevision,
    MoveChanges, MoveRef, MoveRevision, MoveSource, MutationResult, RenameBranch, StoreRef,
    TrackBranch, TreePath, UndoOperation, UntrackBranch,
};

macro_rules! precondition {
    ($($args:tt)*) => {
        return Ok(MutationResult::PreconditionError { message: format!($($args)*) })
    }
}

impl Mutation for AbandonRevisions {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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
            let commit = tx.repo().store().get_commit(id)
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

impl Mutation for BackoutRevisions {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        if self.ids.len() != 1 {
            precondition!("Not implemented for >1 rev");
        }

        let mut tx = ws.start_transaction()?;

        let working_copy = ws.get_commit(ws.wc_id())?;
        let reverted = ws.resolve_multiple_changes(self.ids)?;
        let reverted_parents: Result<Vec<_>, BackendError> = reverted[0].parents().collect();

        let old_base_tree = rewrite::merge_commit_trees(tx.repo(), &reverted_parents?)?;
        let new_base_tree = working_copy.tree()?;
        let old_tree = reverted[0].tree()?;
        let new_tree = new_base_tree.merge(&old_tree, &old_base_tree)?;

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

impl Mutation for CheckoutRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let edited = ws.resolve_single_change(&self.id)?;

        if ws.check_immutable(vec![edited.id().clone()])? {
            precondition!("Revision is immutable");
        }

        if edited.id() == ws.wc_id() {
            return Ok(MutationResult::Unchanged);
        }

        tx.repo_mut().edit(ws.id().clone(), &edited)?;

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

impl Mutation for CreateRevision {
    fn execute<'a>(self: Box<Self>, ws: &'a mut WorkspaceSession) -> Result<MutationResult> {
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
        let merged_tree = rewrite::merge_commit_trees(tx.repo(), &parent_commits)?;

        let new_commit = tx
            .repo_mut()
            .new_commit(parent_ids?, merged_tree.id())
            .write()?;

        tx.repo_mut().edit(ws.id().clone(), &new_commit)?;

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

impl Mutation for DescribeRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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

impl Mutation for DuplicateRevisions {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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

impl Mutation for InsertRevision {
    fn execute<'a>(self: Box<Self>, ws: &'a mut WorkspaceSession) -> Result<MutationResult> {
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
        let target =
            rewrite::rebase_commit(tx.repo_mut(), target, vec![after_id])?;
        rewrite::rebase_commit(
            tx.repo_mut(),
            before,
            vec![target.id().clone()],
        )?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

impl Mutation for MoveRevision {
    fn execute<'a>(self: Box<Self>, ws: &'a mut WorkspaceSession) -> Result<MutationResult> {
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
        rewrite::rebase_commit(tx.repo_mut(), target, parent_ids)?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

impl Mutation for MoveSource {
    fn execute<'a>(self: Box<Self>, ws: &'a mut WorkspaceSession) -> Result<MutationResult> {
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
        rewrite::rebase_commit(tx.repo_mut(), target, parent_ids)?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

impl Mutation for MoveChanges {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from = ws.resolve_single_change(&self.from_id)?;
        let mut to = ws.resolve_single_commit(&self.to_id)?;
        let matcher = build_matcher(&self.paths);

        if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
            precondition!("Revisions are immutable");
        }

        // construct a split tree and a remainder tree by copying changes from child to parent and from parent to child
        let from_tree = from.tree()?;
        let from_parents: Result<Vec<_>, _> = from.parents().collect();
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &from_parents?)?;
        let split_tree_id = rewrite::restore_tree(&from_tree, &parent_tree, matcher.as_ref())?;
        let split_tree = tx.repo().store().get_root_tree(&split_tree_id)?;
        let remainder_tree_id = rewrite::restore_tree(&parent_tree, &from_tree, matcher.as_ref())?;
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
        if tx.repo().index().is_ancestor(from.id(), to.id()) {
            let mut rebase_map = std::collections::HashMap::new();
            tx.repo_mut().rebase_descendants_with_options(&RebaseOptions::default(), |old_commit, rebased_commit| {
                rebase_map.insert(
                    old_commit.id().clone(),
                    match rebased_commit {
                        RebasedCommit::Rewritten(new_commit) => new_commit.id().clone(),
                        RebasedCommit::Abandoned { parent_id } => parent_id,
                    },
                );
            })?;
            let rebased_to_id = rebase_map
                .get(to.id())
                .ok_or_else(|| anyhow!("descendant to_commit not found in rebase map"))?
                .clone();
            to = tx.repo().store().get_commit(&rebased_to_id)?;
        }

        // apply changes to destination
        let to_tree = to.tree()?;
        let new_to_tree = to_tree.merge(&parent_tree, &split_tree)?;
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

impl Mutation for CopyChanges {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from_tree = ws.resolve_single_commit(&self.from_id)?.tree()?;
        let to = ws.resolve_single_change(&self.to_id)?;
        let matcher = build_matcher(&self.paths);

        if ws.check_immutable(vec![to.id().clone()])? {
            precondition!("Revisions are immutable");
        }

        // construct a restore tree - the destination with some portions overwritten by the source
        let to_tree = to.tree()?;
        let new_to_tree_id = rewrite::restore_tree(&from_tree, &to_tree, matcher.as_ref())?;
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

impl Mutation for TrackBranch {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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

                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_bookmark(&branch_name, &remote_name);

                if remote_ref.is_tracking() {
                    precondition!("{branch_name}@{remote_name} is already tracked");
                }

                tx.repo_mut()
                    .track_remote_bookmark(&branch_name, &remote_name);

                match ws.finish_transaction(tx, format!("track remote bookmark {}", branch_name))? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

impl Mutation for UntrackBranch {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let mut untracked = Vec::new();
        match self.r#ref {
            StoreRef::Tag { tag_name } => {
                precondition!("{} is a tag and cannot be untracked", tag_name);
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                // untrack all remotes
                for ((name, remote), remote_ref) in ws.view().remote_bookmarks_matching(
                    &StringPattern::exact(branch_name),
                    &StringPattern::everything(),
                ) {
                    if remote != REMOTE_NAME_FOR_LOCAL_GIT_REPO && remote_ref.is_tracking() {
                        tx.repo_mut().untrack_remote_bookmark(name, remote);
                        untracked.push(format!("{name}@{remote}"));
                    }
                }
            }
            StoreRef::RemoteBookmark {
                branch_name,
                remote_name,
                ..
            } => {
                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_bookmark(&branch_name, &remote_name);

                if !remote_ref.is_tracking() {
                    precondition!("{branch_name}@{remote_name} is not tracked");
                }

                tx.repo_mut()
                    .untrack_remote_bookmark(&branch_name, &remote_name);
                untracked.push(format!("{branch_name}@{remote_name}"));
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

impl Mutation for RenameBranch {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let old_name = self.r#ref.as_branch()?;

        let ref_target = ws.view().get_local_bookmark(old_name).clone();
        if ref_target.is_absent() {
            precondition!("No such bookmark: {}", old_name);
        }

        if ws.view().get_local_bookmark(&self.new_name).is_present() {
            precondition!("Bookmark already exists: {}", &self.new_name);
        }

        let mut tx = ws.start_transaction()?;

        tx.repo_mut()
            .set_local_bookmark_target(&self.new_name, ref_target);
        tx.repo_mut()
            .set_local_bookmark_target(old_name, RefTarget::absent());

        match ws.finish_transaction(tx, format!("rename {} to {}", old_name, self.new_name))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

impl Mutation for CreateRef {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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
                let existing_branch = ws.view().get_local_bookmark(&branch_name);
                if existing_branch.is_present() {
                    precondition!("{} already exists", branch_name);
                }

                tx.repo_mut().set_local_bookmark_target(
                    &branch_name,
                    RefTarget::normal(commit.id().clone()),
                );

                match ws.finish_transaction(
                    tx,
                    format!(
                        "create {} pointing to commit {}",
                        branch_name,
                        ws.format_commit_id(commit.id()).hex
                    ),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name, .. } => {
                let existing_tag = ws.view().get_tag(&tag_name);
                if existing_tag.is_present() {
                    precondition!("{} already exists", tag_name);
                }

                tx.repo_mut()
                    .set_tag_target(&tag_name, RefTarget::normal(commit.id().clone()));

                match ws.finish_transaction(
                    tx,
                    format!(
                        "create {} pointing to commit {}",
                        tag_name,
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

impl Mutation for DeleteRef {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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

                tx.repo_mut()
                    .set_remote_bookmark(&branch_name, &remote_name, remote_ref);

                match ws
                    .finish_transaction(tx, format!("forget {}@{}", branch_name, remote_name))?
                {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::LocalBookmark { branch_name, .. } => {
                let mut tx = ws.start_transaction()?;

                tx.repo_mut()
                    .set_local_bookmark_target(&branch_name, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget {}", branch_name))? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name } => {
                let mut tx = ws.start_transaction()?;

                tx.repo_mut().set_tag_target(&tag_name, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget tag {}", tag_name))? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

// does not currently enforce fast-forwards
impl Mutation for MoveRef {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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
                let old_target = ws.view().get_local_bookmark(&branch_name);
                if old_target.is_absent() {
                    precondition!("No such bookmark: {branch_name}");
                }

                tx.repo_mut().set_local_bookmark_target(
                    &branch_name,
                    RefTarget::normal(commit.id().clone()),
                );

                match ws.finish_transaction(
                    tx,
                    format!("point {} to commit {}", branch_name, commit.id().hex()),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name } => {
                let old_target = ws.view().get_tag(&tag_name);
                if old_target.is_absent() {
                    precondition!("No such tag: {tag_name}");
                }

                tx.repo_mut()
                    .set_tag_target(&tag_name, RefTarget::normal(commit.id().clone()));

                match ws.finish_transaction(
                    tx,
                    format!("point {} to commit {}", tag_name, commit.id().hex()),
                )? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
        }
    }
}

impl Mutation for GitPush {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let git_repo = match ws.git_repo()? {
            Some(git_repo) => git_repo,
            None => precondition!("No git backend"),
        };

        // determine bookmarks to push, recording the old and new commits
        let mut remote_branch_updates: Vec<(&str, Vec<(String, refs::BookmarkPushUpdate)>)> =
            Vec::new();
        let remote_branch_refs: Vec<_> = match &*self {
            GitPush::AllBookmarks { ref remote_name } => {
                let mut branch_updates = Vec::new();
                for (branch_name, targets) in ws.view().local_remote_bookmarks(&remote_name) {
                    if !targets.remote_ref.is_tracking() {
                        continue;
                    }

                    match classify_branch_push(branch_name, &remote_name, targets) {
                        Err(message) => return Ok(MutationResult::PreconditionError { message }),
                        Ok(None) => (),
                        Ok(Some(update)) => branch_updates.push((branch_name.to_owned(), update)),
                    }
                }
                remote_branch_updates.push((remote_name, branch_updates));

                ws.view().remote_bookmarks(&remote_name).collect()
            }
            GitPush::AllRemotes { branch_ref } => {
                let branch_name = branch_ref.as_branch()?;

                let mut remote_branch_refs = Vec::new();
                for (remote_name, group) in ws
                    .view()
                    .all_remote_bookmarks()
                    .filter_map(|((branch, remote), remote_ref)| {
                        if remote_ref.is_tracking() && branch == branch_name {
                            Some((remote, remote_ref))
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
                            local_target: ws.view().get_local_bookmark(branch_name),
                            remote_ref,
                        };
                        match classify_branch_push(branch_name, &remote_name, targets) {
                            Err(message) => {
                                return Ok(MutationResult::PreconditionError { message })
                            }
                            Ok(None) => (),
                            Ok(Some(update)) => {
                                branch_updates.push((branch_name.to_owned(), update))
                            }
                        }
                        remote_branch_refs.push((remote_name, remote_ref));
                    }
                    remote_branch_updates.push((remote_name, branch_updates));
                }

                remote_branch_refs
            }
            GitPush::RemoteBookmark {
                ref remote_name,
                ref branch_ref,
            } => {
                let branch_name = branch_ref.as_branch()?;
                let local_target = ws.view().get_local_bookmark(branch_name);
                let remote_ref = ws.view().get_remote_bookmark(branch_name, remote_name);

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
                            .push((remote_name, vec![(branch_name.to_owned(), update)]));
                    }
                }

                vec![(
                    branch_name,
                    ws.view().get_remote_bookmark(branch_name, &remote_name),
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
            if commit.has_conflict()? {
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
            let git_settings = ws.data.settings.git_settings()?;

            ws.session.callbacks.with_git(tx.repo_mut(), &|repo, cb| {
                Ok(git::push_branches(
                    repo,
                    &git_settings,
                    &remote_name,
                    &targets,
                    cb,
                )?)
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

impl Mutation for GitFetch {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let git_repo = match ws.git_repo()? {
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
                for remote_name in git_repo
                    .remotes()?
                    .into_iter()
                    .filter_map(|remote| remote.map(|remote| remote.to_owned()))
                {
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
        let git_settings = ws.data.settings.git_settings()?;

        for (remote_name, pattern) in remote_patterns {
            ws.session.callbacks.with_git(tx.repo_mut(), &|repo, cb| {
                let mut fetcher = git::GitFetch::new(repo, &git_settings)?;
                fetcher.fetch(&remote_name, &[pattern.clone().map(StringPattern::exact).unwrap_or_else(StringPattern::everything)], cb, None)
                    .context("failed to fetch")?;
                Ok(())
            })?;
        }

        match ws.finish_transaction(tx, format!("fetch from git remote(s)"))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

// this is another case where it would be nice if we could reuse jj-cli's error messages
impl Mutation for UndoOperation {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
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
        tx.repo_mut().merge(&head_repo, &parent_repo);
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

fn build_matcher(paths: &Vec<TreePath>) -> Box<dyn Matcher> {
    if paths.is_empty() {
        Box::new(EverythingMatcher)
    } else {
        Box::new(FilesMatcher::new(
            paths
                .iter()
                .map(|p| RepoPath::from_internal_string(&p.repo_path)),
        ))
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
