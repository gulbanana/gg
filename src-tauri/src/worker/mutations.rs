use anyhow::Result;
use itertools::Itertools;
use jj_lib::{
    backend::BackendError, commit::Commit, object_id::ObjectId, op_walk, protos::working_copy,
    repo::Repo, revset::RevsetIteratorExt, rewrite::merge_commit_trees,
};

use crate::{
    gui_util::WorkspaceSession,
    messages::{
        AbandonRevision, CheckoutRevision, CopyChanges, CreateRevision, DescribeRevision,
        DuplicateRevision, MoveChanges, MutationResult, UndoOperation,
    },
};

use super::Mutation;

impl Mutation for CheckoutRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let edited_commit = ws.evaluate_rev_str(&self.change_id.hex)?;

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
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        let mut tx = ws.start_transaction()?;

        let mut parents_expr = self.parent_change_ids[0].hex.clone();
        for parent_id in self.parent_change_ids.iter().skip(1) {
            parents_expr.push_str("|");
            parents_expr.push_str(&parent_id.hex);
        }

        let parents_revset = ws.evaluate_revset_str(&parents_expr)?;
        let parent_ids = parents_revset.iter().collect_vec();
        let parent_commits: Result<Vec<Commit>, BackendError> =
            parents_revset.iter().commits(tx.repo().store()).collect();

        let merged_tree = merge_commit_trees(tx.repo(), &parent_commits?)?;

        drop(parents_revset);

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

        let commit = ws.evaluate_rev_str(&self.change_id.hex)?;

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

impl Mutation for DuplicateRevision {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
        todo!("DuplicateRevision")
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
