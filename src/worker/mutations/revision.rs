use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    backend::BackendError,
    commit::{Commit, conflict_label_for_commits},
    merge::Merge,
    merged_tree::MergedTree,
    object_id::ObjectId as ObjectIdTrait,
    rewrite::{self, RebaseOptions, RebasedCommit},
};

use super::precondition;

use crate::{
    messages::mutations::{
        AbandonRevisions, AdoptRevision, BackoutRevisions, CheckoutRevision, CreateRevision,
        CreateRevisionBetween, DescribeRevision, DuplicateRevisions, InsertRevisions,
        MoveRevisions, MutationOptions, MutationResult,
    },
    worker::{Mutation, gui_util::WorkspaceSession},
};

#[async_trait(?Send)]
impl Mutation for AbandonRevisions {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (commits, is_immutable) = ws.resolve_change_set(&self.set, true)?;
        if is_immutable && !options.ignore_immutable {
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
impl Mutation for AdoptRevision {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
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

        if ws.check_immutable(vec![target.id().clone()])? && !options.ignore_immutable {
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
impl Mutation for BackoutRevisions {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
        let wc_tree_ids = wc_tree.tree_ids().clone();

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

        // three-way merge: wc + (parent - reverted)
        let new_wc_tree = MergedTree::merge(Merge::from_vec(vec![
            (wc_tree, wc_label),
            (reverted_tree, reverted_label),
            (parent_tree, parent_label),
        ]))
        .await?;

        // changes already present in working copy
        if new_wc_tree.tree_ids() == &wc_tree_ids {
            return Ok(MutationResult::Unchanged);
        }

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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let edited = ws.resolve_change_id(&self.id)?;

        if ws.check_immutable(vec![edited.id().clone()])? && !options.ignore_immutable {
            precondition!("Revision is immutable");
        }

        if edited.id() == ws.wc_id() {
            return Ok(MutationResult::Unchanged);
        }

        tx.repo_mut().edit(ws.name().to_owned(), &edited)?;

        match ws.finish_transaction_for_edit(
            tx,
            format!("edit commit {}", edited.id().hex()),
            options.ignore_immutable,
        )? {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
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
        if ws.check_immutable(vec![before_commit.id().clone()])? && !options.ignore_immutable {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let described = ws.resolve_change_id(&self.id)?;

        if ws.check_immutable(vec![described.id().clone()])? && !options.ignore_immutable {
            precondition!("Revision {} is immutable", self.id.change.prefix);
        }

        if self.new_description == described.description() && !self.reset_author {
            return Ok(MutationResult::Unchanged);
        }

        if self.reset_author {
            let missing_name = ws.data.workspace_settings.user_name().is_empty();
            let missing_email = ws.data.workspace_settings.user_email().is_empty();
            if missing_name || missing_email {
                let field = match (missing_name, missing_email) {
                    (true, true) => "Name and email not configured.",
                    (true, false) => "Name not configured.",
                    (false, true) => "Email not configured.",
                    _ => unreachable!(),
                };
                precondition!(
                    "{field} Set them with `jj config set --user user.name \"Some One\"` \
                     and `jj config set --user user.email \"someone@example.com\"`."
                );
            }
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (targets, is_immutable) = ws.resolve_change_set(&self.set, true)?;
        if is_immutable && !options.ignore_immutable {
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

        if ws.check_immutable([before.id().clone()])? && !options.ignore_immutable {
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
            rewrite::rebase_commit(tx.repo_mut(), oldest.clone(), vec![after_id.clone()]).await?;

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

        // replace after as a parent of before, preserving other parents
        let new_before_parent_ids: Vec<_> = before
            .parent_ids()
            .iter()
            .map(|parent_id| {
                let mapped_parent_id = rebased_children.get(parent_id).unwrap_or(parent_id);
                if mapped_parent_id == &after_id {
                    new_newest_id.clone()
                } else {
                    mapped_parent_id.clone()
                }
            })
            .collect();

        // rebase graph suffix onto the end of the inserted range
        rewrite::rebase_commit(tx.repo_mut(), before, new_before_parent_ids).await?;

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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let (targets, is_immutable) = ws.resolve_change_set(&self.set, true)?;
        if is_immutable && !options.ignore_immutable {
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
