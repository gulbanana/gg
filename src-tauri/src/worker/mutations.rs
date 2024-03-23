use std::{collections::HashSet, fmt::Display};

use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    backend::CommitId,
    commit::Commit,
    git::{self, GitBranchPushTargets, REMOTE_NAME_FOR_LOCAL_GIT_REPO},
    matchers::{EverythingMatcher, FilesMatcher, Matcher},
    object_id::ObjectId,
    op_store::{RefTarget, RemoteRef, RemoteRefState},
    op_walk,
    refs::{self, BranchPushAction, BranchPushUpdate, LocalAndRemoteRef},
    repo::Repo,
    repo_path::RepoPath,
    revset::{self, RevsetIteratorExt},
    rewrite,
    settings::UserSettings,
    str_util::StringPattern,
};

use super::{gui_util::WorkspaceSession, Mutation};
use crate::messages::{
    AbandonRevisions, CheckoutRevision, CopyChanges, CreateRef, CreateRevision, DeleteRef,
    DescribeRevision, DuplicateRevisions, GitFetch, GitPush, InsertRevision, MoveChanges, MoveRef,
    MoveRevision, MoveSource, MutationResult, RenameBranch, StoreRef, TrackBranch, TreePath,
    UndoOperation, UntrackBranch,
};

macro_rules! precondition {
    ($($args:tt)*) => {
        return Ok(MutationResult::PreconditionError { message: format!($($args)*) })
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

        tx.mut_repo().edit(ws.id().clone(), &edited)?;

        match ws.finish_transaction(tx, format!("edit commit {}", edited.id().hex()))? {
            Some(new_status) => {
                let new_selection = ws.format_header(&edited, None)?;
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

        let parent_ids = parents_revset.iter().collect_vec();
        let parent_commits = ws.resolve_multiple(parents_revset)?;
        let merged_tree = rewrite::merge_commit_trees(tx.repo(), &parent_commits)?;

        let new_commit = tx
            .mut_repo()
            .new_commit(&ws.settings, parent_ids, merged_tree.id())
            .write()?;

        tx.mut_repo().edit(ws.id().clone(), &new_commit)?;

        match ws.finish_transaction(tx, "new empty commit")? {
            Some(new_status) => {
                let new_selection = ws.format_header(&new_commit, None)?;
                Ok(MutationResult::UpdatedSelection {
                    new_status,
                    new_selection,
                })
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
        let after = rebased_children
            .get(after.id())
            .map_or(Ok(after.clone()), |rebased_before_id| {
                tx.repo().store().get_commit(rebased_before_id)
            })?;

        // rebase the target (which now has no children), then the new post-target tree atop it
        let rebased_id = target.id().hex();
        let target = rewrite::rebase_commit(&ws.settings, tx.mut_repo(), &target, &[after])?;
        rewrite::rebase_commit(&ws.settings, tx.mut_repo(), &before, &[target])?;

        match ws.finish_transaction(tx, format!("rebase commit {}", rebased_id))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
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
            .mut_repo()
            .rewrite_commit(&ws.settings, &described)
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

        let clonees = ws.resolve_multiple_changes(self.ids)?;
        let mut clones: IndexMap<Commit, Commit> = IndexMap::new();

        let base_repo = tx.base_repo().clone();
        let store = base_repo.store();
        let mut_repo = tx.mut_repo();

        for clonee_id in base_repo
            .index()
            .topo_order(&mut clonees.iter().map(|c| c.id())) // ensures that parents are duplicated first
            .into_iter()
        {
            let clonee = store.get_commit(&clonee_id)?;
            let clone_parents = clonee
                .parents()
                .iter()
                .map(|parent| {
                    if let Some(cloned_parent) = clones.get(parent) {
                        cloned_parent
                    } else {
                        parent
                    }
                    .id()
                    .clone()
                })
                .collect();
            let clone = mut_repo
                .rewrite_commit(&ws.settings, &clonee)
                .generate_new_change_id()
                .set_parents(clone_parents)
                .write()?;
            clones.insert(clonee, clone);
        }

        match ws.finish_transaction(tx, format!("duplicating {} commit(s)", clonees.len()))? {
            Some(new_status) => {
                if clonees.len() == 1 {
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
            tx.mut_repo().record_abandoned_commit(id.clone());
        }
        tx.mut_repo().rebase_descendants(&ws.settings)?;

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
        let parents: Vec<_> = parents
            .iter()
            .map(|new_parent| {
                rebased_children
                    .get(new_parent.id())
                    .map_or(Ok(new_parent.clone()), |rebased_new_parent_id| {
                        tx.repo().store().get_commit(rebased_new_parent_id)
                    })
            })
            .try_collect()?;

        // rebase the target itself
        let rebased_id = target.id().hex();
        rewrite::rebase_commit(&ws.settings, tx.mut_repo(), &target, &parents)?;

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
        let parents = ws.resolve_multiple_commits(&self.parent_ids)?;

        if ws.check_immutable(vec![target.id().clone()])? {
            precondition!("Revision {} is immutable", self.id.change.prefix);
        }

        // just rebase the target, which will also rebase its descendants
        let rebased_id = target.id().hex();
        rewrite::rebase_commit(&ws.settings, tx.mut_repo(), &target, &parents)?;

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
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &from.parents())?;
        let split_tree_id = rewrite::restore_tree(&from_tree, &parent_tree, matcher.as_ref())?;
        let split_tree = tx.repo().store().get_root_tree(&split_tree_id)?;
        let remainder_tree_id = rewrite::restore_tree(&parent_tree, &from_tree, matcher.as_ref())?;
        let remainder_tree = tx.repo().store().get_root_tree(&remainder_tree_id)?;

        // abandon or rewrite source
        let abandon_source = remainder_tree.id() == parent_tree.id();
        if abandon_source {
            tx.mut_repo().record_abandoned_commit(from.id().clone());
        } else {
            tx.mut_repo()
                .rewrite_commit(&ws.settings, &from)
                .set_tree_id(remainder_tree.id().clone())
                .write()?;
        }

        // rebase descendants of source, which may include destination
        if tx.repo().index().is_ancestor(from.id(), to.id()) {
            let rebase_map = tx.mut_repo().rebase_descendants_return_map(&ws.settings)?;
            let rebased_to_id = rebase_map
                .get(to.id())
                .ok_or(anyhow!("descendant to_commit not found in rebase map"))?
                .clone();
            to = tx.mut_repo().store().get_commit(&rebased_to_id)?;
        }

        // apply changes to destination
        let to_tree = to.tree()?;
        let new_to_tree = to_tree.merge(&parent_tree, &split_tree)?;
        let description = combine_messages(&from, &to, abandon_source);
        tx.mut_repo()
            .rewrite_commit(&ws.settings, &to)
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
            tx.mut_repo()
                .rewrite_commit(&ws.settings, &to)
                .set_tree_id(new_to_tree_id)
                .write()?;

            tx.mut_repo().rebase_descendants(&ws.settings)?;

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
            StoreRef::LocalBranch { branch_name, .. } => {
                precondition!("{} is a local branch and cannot be tracked", branch_name);
            }
            StoreRef::RemoteBranch {
                branch_name,
                remote_name,
                ..
            } => {
                let mut tx = ws.start_transaction()?;

                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_branch(&branch_name, &remote_name);

                if remote_ref.is_tracking() {
                    precondition!("{branch_name}@{remote_name} is already tracked");
                }

                tx.mut_repo()
                    .track_remote_branch(&branch_name, &remote_name);

                match ws.finish_transaction(tx, format!("track remote branch {}", branch_name))? {
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
            StoreRef::LocalBranch { branch_name, .. } => {
                // untrack all remotes
                for ((name, remote), remote_ref) in ws.view().remote_branches_matching(
                    &StringPattern::exact(branch_name),
                    &StringPattern::everything(),
                ) {
                    if remote != REMOTE_NAME_FOR_LOCAL_GIT_REPO && remote_ref.is_tracking() {
                        tx.mut_repo().untrack_remote_branch(name, remote);
                        untracked.push(format!("{name}@{remote}"));
                    }
                }
            }
            StoreRef::RemoteBranch {
                branch_name,
                remote_name,
                ..
            } => {
                let remote_ref: &jj_lib::op_store::RemoteRef =
                    ws.view().get_remote_branch(&branch_name, &remote_name);

                if !remote_ref.is_tracking() {
                    precondition!("{branch_name}@{remote_name} is not tracked");
                }

                tx.mut_repo()
                    .untrack_remote_branch(&branch_name, &remote_name);
                untracked.push(format!("{branch_name}@{remote_name}"));
            }
        }

        match ws.finish_transaction(
            tx,
            format!("untrack remote {}", combine_branches(&untracked)),
        )? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

impl Mutation for RenameBranch {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let old_name = self.r#ref.as_branch()?;

        let ref_target = ws.view().get_local_branch(old_name).clone();
        if ref_target.is_absent() {
            precondition!("No such branch: {}", old_name);
        }

        if ws.view().get_local_branch(&self.new_name).is_present() {
            precondition!("Branch already exists: {}", &self.new_name);
        }

        let mut tx = ws.start_transaction()?;

        tx.mut_repo()
            .set_local_branch_target(&self.new_name, ref_target);
        tx.mut_repo()
            .set_local_branch_target(old_name, RefTarget::absent());

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
            StoreRef::RemoteBranch {
                branch_name,
                remote_name,
                ..
            } => {
                precondition!(
                    "{}@{} is a remote branch and cannot be created",
                    branch_name,
                    remote_name
                );
            }
            StoreRef::LocalBranch { branch_name, .. } => {
                let existing_branch = ws.view().get_local_branch(&branch_name);
                if existing_branch.is_present() {
                    precondition!("{} already exists", branch_name);
                }

                tx.mut_repo()
                    .set_local_branch_target(&branch_name, RefTarget::normal(commit.id().clone()));

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

                tx.mut_repo()
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
            StoreRef::RemoteBranch {
                branch_name,
                remote_name,
                ..
            } => {
                let mut tx = ws.start_transaction()?;

                // forget the branch entirely - when target is absent, it's removed from the view
                let remote_ref = RemoteRef {
                    target: RefTarget::absent(),
                    state: RemoteRefState::New,
                };

                tx.mut_repo()
                    .set_remote_branch(&branch_name, &remote_name, remote_ref);

                match ws
                    .finish_transaction(tx, format!("forget {}@{}", branch_name, remote_name))?
                {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::LocalBranch { branch_name, .. } => {
                let mut tx = ws.start_transaction()?;

                tx.mut_repo()
                    .set_local_branch_target(&branch_name, RefTarget::absent());

                match ws.finish_transaction(tx, format!("forget {}", branch_name))? {
                    Some(new_status) => Ok(MutationResult::Updated { new_status }),
                    None => Ok(MutationResult::Unchanged),
                }
            }
            StoreRef::Tag { tag_name } => {
                let mut tx = ws.start_transaction()?;

                tx.mut_repo().set_tag_target(&tag_name, RefTarget::absent());

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
            StoreRef::RemoteBranch {
                branch_name,
                remote_name,
                ..
            } => {
                precondition!("Branch is remote: {branch_name}@{remote_name}")
            }
            StoreRef::LocalBranch { branch_name, .. } => {
                let old_target = ws.view().get_local_branch(&branch_name);
                if old_target.is_absent() {
                    precondition!("No such branch: {branch_name}");
                }

                tx.mut_repo()
                    .set_local_branch_target(&branch_name, RefTarget::normal(commit.id().clone()));

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

                tx.mut_repo()
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

        // determine branches to push, recording the old and new commits
        let mut remote_branch_updates: Vec<(
            &str,
            Vec<(String, refs::BranchPushUpdate)>,
            HashSet<String>,
        )> = Vec::new();
        let remote_branch_refs: Vec<_> = match &*self {
            GitPush::AllBranches { ref remote_name } => {
                let mut branch_updates = Vec::new();
                for (branch_name, targets) in ws.view().local_remote_branches(&remote_name) {
                    if !targets.remote_ref.is_tracking() {
                        continue;
                    }

                    match classify_branch_push(branch_name, &remote_name, targets) {
                        Err(message) => return Ok(MutationResult::PreconditionError { message }),
                        Ok(None) => (),
                        Ok(Some(update)) => branch_updates.push((branch_name.to_owned(), update)),
                    }
                }
                remote_branch_updates.push((remote_name, branch_updates, HashSet::new()));

                ws.view().remote_branches(&remote_name).collect()
            }
            GitPush::AllRemotes { branch_ref } => {
                let branch_name = branch_ref.as_branch()?;

                let mut remote_branch_refs = Vec::new();
                for (remote_name, group) in ws
                    .view()
                    .all_remote_branches()
                    .filter_map(|((branch, remote), remote_ref)| {
                        if remote_ref.is_tracking() && branch == branch_name {
                            Some((remote, remote_ref))
                        } else {
                            None
                        }
                    })
                    .group_by(|(remote_name, _)| *remote_name)
                    .into_iter()
                {
                    let mut branch_updates = Vec::new();
                    for (_, remote_ref) in group {
                        let targets = LocalAndRemoteRef {
                            local_target: ws.view().get_local_branch(branch_name),
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
                    remote_branch_updates.push((remote_name, branch_updates, HashSet::new()));
                }

                remote_branch_refs
            }
            GitPush::RemoteBranch {
                ref remote_name,
                ref branch_ref,
            } => {
                let branch_name = branch_ref.as_branch()?;
                let local_target = ws.view().get_local_branch(branch_name);
                let remote_ref = ws.view().get_remote_branch(branch_name, remote_name);

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
                        remote_branch_updates.push((
                            remote_name,
                            vec![(branch_name.to_owned(), update)],
                            HashSet::new(),
                        ));
                    }
                }

                vec![(
                    branch_name,
                    ws.view().get_remote_branch(branch_name, &remote_name),
                )]
            }
        };

        // check for conflicts
        let mut new_heads = vec![];
        for (_, branch_updates, force_pushed_branches) in &mut remote_branch_updates {
            for (branch_name, update) in branch_updates {
                if let Some(new_target) = &update.new_target {
                    new_heads.push(new_target.clone());
                    let force = match &update.old_target {
                        None => false,
                        Some(old_target) => !ws.repo().index().is_ancestor(old_target, new_target),
                    };
                    if force {
                        force_pushed_branches.insert(branch_name.to_string());
                    }
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
        for (remote_name, branch_updates, force_pushed_branches) in
            remote_branch_updates.into_iter()
        {
            let targets = GitBranchPushTargets {
                branch_updates,
                force_pushed_branches,
            };

            ws.session.callbacks.with_git(tx.mut_repo(), &|repo, cb| {
                Ok(git::push_branches(
                    repo,
                    &git_repo,
                    &remote_name,
                    &targets,
                    cb,
                )?)
            })?;
        }

        match ws.finish_transaction(
            tx,
            match *self {
                GitPush::AllBranches { remote_name } => {
                    format!("push all tracked branches to git remote {}", remote_name)
                }
                GitPush::AllRemotes { branch_ref } => {
                    format!(
                        "push {} to all tracked git remotes",
                        branch_ref.as_branch()?
                    )
                }
                GitPush::RemoteBranch {
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
            GitFetch::AllBranches { remote_name } => {
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
            GitFetch::RemoteBranch {
                remote_name,
                branch_ref,
            } => {
                let branch_name = branch_ref.as_branch()?;
                remote_patterns.push((remote_name, Some(branch_name.to_owned())));
            }
        }

        for (remote_name, pattern) in remote_patterns {
            ws.session.callbacks.with_git(tx.mut_repo(), &|repo, cb| {
                git::fetch(
                    repo,
                    &git_repo,
                    &remote_name,
                    &[pattern
                        .clone()
                        .map(StringPattern::exact)
                        .unwrap_or_else(StringPattern::everything)],
                    cb,
                    &ws.settings.git_settings(),
                )?;
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
        tx.mut_repo().merge(&head_repo, &parent_repo);
        let restored_view = tx.repo().view().store_view().clone();
        tx.mut_repo().set_view(restored_view);

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

fn combine_branches(branch_names: &[impl Display]) -> String {
    match branch_names {
        [branch_name] => format!("branch {}", branch_name),
        branch_names => format!("branches {}", branch_names.iter().join(", ")),
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
) -> Result<Option<BranchPushUpdate>, String> {
    let push_action = refs::classify_branch_push_action(targets);
    match push_action {
        BranchPushAction::AlreadyMatches => Ok(None),
        BranchPushAction::Update(update) => Ok(Some(update)),
        BranchPushAction::LocalConflicted => Err(format!("Branch {} is conflicted.", branch_name)),
        BranchPushAction::RemoteConflicted => Err(format!(
            "Branch {}@{} is conflicted. Try fetching first.",
            branch_name, remote_name
        )),
        BranchPushAction::RemoteUntracked => Err(format!(
            "Non-tracking remote branch {}@{} exists. Try tracking it first.",
            branch_name, remote_name
        )),
    }
}
