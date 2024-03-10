use anyhow::{anyhow, Result};
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    commit::Commit,
    matchers::{EverythingMatcher, FilesMatcher, Matcher},
    object_id::ObjectId,
    op_walk,
    repo::Repo,
    repo_path::RepoPath,
    rewrite,
};

use crate::{
    gui_util::WorkspaceSession,
    messages::{
        AbandonRevisions, CheckoutRevision, CopyChanges, CreateRevision, DescribeRevision,
        DuplicateRevisions, MoveChanges, MutationResult, TrackBranch, TreePath, UndoOperation,
        UntrackBranch,
    },
};

use super::Mutation;

impl Mutation for CheckoutRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let edited = ws.resolve_single_id(&self.change_id)?;

        if ws.check_immutable(vec![edited.id().clone()])? {
            return Ok("Revision is immutable".into());
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
    fn execute<'a>(
        self: Box<CreateRevision>,
        ws: &'a mut WorkspaceSession,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let parents_revset = ws.evaluate_revset_ids(&self.parent_change_ids)?;

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

impl Mutation for DescribeRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let described = ws.resolve_single_id(&self.change_id)?;

        if ws.check_immutable(vec![described.id().clone()])? {
            return Ok("Revision is immutable".into());
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

        let clonees = ws.resolve_multiple_ids(&self.change_ids)?;
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

        let abandoned_revset = ws.evaluate_revset_ids(&self.change_ids)?;
        let abandoned_ids = abandoned_revset.iter().collect_vec();

        if ws.check_immutable(abandoned_ids.clone())? {
            return Ok("Revisions are immutable".into());
        }

        for id in &abandoned_ids {
            tx.mut_repo().record_abandoned_commit(id.clone());
        }
        tx.mut_repo().rebase_descendants(&ws.settings)?;

        drop(abandoned_revset);

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

impl Mutation for MoveChanges {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let from = ws.resolve_single_id(&self.from_change_id)?;
        let mut to = ws.resolve_single_id(&self.to_id)?;
        let matcher = build_matcher(&self.paths);

        if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
            return Ok("Revisions are immutable".into());
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

        let from_tree = ws.resolve_single_id(&self.from_change_id)?.tree()?;
        let to = ws.resolve_single_id(&self.to_id)?;
        let matcher = build_matcher(&self.paths);

        if ws.check_immutable(vec![to.id().clone()])? {
            return Ok("Revisions are immutable".into());
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
    fn execute(
        self: Box<Self>,
        _ws: &mut WorkspaceSession,
    ) -> Result<crate::messages::MutationResult> {
        Ok("TrackBranch unimplemented".into())
    }
}

impl Mutation for UntrackBranch {
    fn execute(
        self: Box<Self>,
        _ws: &mut WorkspaceSession,
    ) -> Result<crate::messages::MutationResult> {
        Ok("UntrackBranch unimplemented".into())
    }
}

// this is another case where it would be nice if we could reuse jj-cli's error messages
impl Mutation for UndoOperation {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let head_op = op_walk::resolve_op_with_repo(ws.repo(), "@")?;
        let mut parent_ops = head_op.parents();

        let Some(parent_op) = parent_ops.next().transpose()? else {
            return Ok("Cannot undo repo initialization".into());
        };

        if parent_ops.next().is_some() {
            return Ok("Cannot undo a merge operation".into());
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
