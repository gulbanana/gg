use anyhow::Result;
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    commit::Commit, object_id::ObjectId, op_walk,
    repo::Repo, rewrite,
};

use crate::{
    gui_util::WorkspaceSession,
    messages::{
        AbandonRevision, CheckoutRevision, CopyChanges, CreateRevision, DescribeRevision, DuplicateRevisions, MoveChanges, MutationResult, UndoOperation
    },
};

use super::Mutation;

impl Mutation for CheckoutRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let edited_commit = ws.resolve_single_id(&self.change_id)?;

        if ws.check_immutable(edited_commit.id().clone())? {
            Ok(format!(
                "Change {}{} is immutable",
                self.change_id.prefix, self.change_id.rest
            )
            .into())
        } else if edited_commit.id() == ws.wc_id() {
            Ok(MutationResult::Unchanged)
        } else {
            tx.mut_repo().edit(ws.id().clone(), &edited_commit)?;

            match ws.finish_transaction(tx, format!("edit commit {}", edited_commit.id().hex()))? {
                Some(new_status) => {
                    let new_selection = ws.format_header(&edited_commit, None)?;
                    Ok(MutationResult::UpdatedSelection {
                        new_status,
                        new_selection,
                    })
                }
                None => Ok(MutationResult::Unchanged),
            }
        }
    }
}

impl Mutation for CreateRevision {
    fn execute<'a>(self: Box<CreateRevision>, ws: &'a mut WorkspaceSession) -> Result<MutationResult> {
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

        let commit = ws.resolve_single_id(&self.change_id)?;

        if ws.check_immutable(commit.id().clone())? {
            Ok(format!(
                "Change {}{} is immutable",
                self.change_id.prefix, self.change_id.rest
            )
            .into())
        } else if self.new_description == commit.description() && !self.reset_author {
            Ok(MutationResult::Unchanged)
        } else {
            let mut commit_builder = tx
                .mut_repo()
                .rewrite_commit(&ws.settings, &commit)
                .set_description(self.new_description);

            if self.reset_author {
                let new_author = commit_builder.committer().clone();
                commit_builder = commit_builder.set_author(new_author);
            }

            commit_builder.write()?;

            match ws.finish_transaction(tx, format!("describe commit {}", commit.id().hex()))? {
                Some(new_status) => Ok(MutationResult::Updated { new_status }),
                None => Ok(MutationResult::Unchanged),
            }
        }
    }
}

impl Mutation for DuplicateRevisions {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx: jj_lib::transaction::Transaction = ws.start_transaction()?;

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
            let clonee = store.get_commit(&clonee_id).unwrap();
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
                    let new_commit = clones.get_index(0).unwrap().1;
                    let new_selection = ws.format_header(new_commit, None)?;
                    Ok(MutationResult::UpdatedSelection {
                        new_status,
                        new_selection,
                    })
                } else {
                    Ok(MutationResult::Updated {
                        new_status,
                    })
                }
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

impl Mutation for AbandonRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        todo!("AbandonRevision")
    }
}

impl Mutation for MoveChanges {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        todo!("MoveChanges")
    }
}

impl Mutation for CopyChanges {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        todo!("CopyChanges")
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
