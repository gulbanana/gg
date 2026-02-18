use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use indexmap::IndexMap;
use itertools::Itertools;
use jj_lib::{
    backend::CommitId,
    commit::{Commit, conflict_label_for_commits},
    merge::Merge,
    merged_tree::MergedTree,
    object_id::ObjectId as ObjectIdTrait,
    repo::Repo,
    revset::{RevsetExpression, RevsetIteratorExt},
    rewrite::{self, RebaseOptions, RebasedCommit},
    transaction::Transaction,
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
        tx.repo_mut().rebase_descendants().await?;

        let transaction_description = if commits.len() == 1 {
            format!("abandon commit {}", commits[0].id().hex())
        } else {
            format!(
                "abandon commit {} and {} more",
                commits[0].id().hex(),
                commits.len() - 1
            )
        };

        match ws.finish_transaction(tx, transaction_description).await? {
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

        match ws
            .finish_transaction(tx, format!("rebase commit {}", rebased_id))
            .await?
        {
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
        let oldest_parents = oldest.parents().await?;
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
            .write()
            .await?;

        let transaction_description = if commits.len() == 1 {
            format!("back out commit {}", newest.id().hex())
        } else {
            format!(
                "back out commit {} and {} more",
                newest.id().hex(),
                commits.len() - 1
            )
        };

        match ws.finish_transaction(tx, transaction_description).await? {
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

        tx.repo_mut().edit(ws.name().to_owned(), &edited).await?;

        match ws
            .finish_transaction_for_edit(
                tx,
                format!("edit commit {}", edited.id().hex()),
                options.ignore_immutable,
            )
            .await?
        {
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
        let new_commit = tx
            .repo_mut()
            .new_commit(parent_ids, merged_tree)
            .write()
            .await?;

        // make it the working copy
        tx.repo_mut()
            .edit(ws.name().to_owned(), &new_commit)
            .await?;

        match ws.finish_transaction(tx, "new empty commit").await? {
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

        let new_commit = tx
            .repo_mut()
            .new_commit(parent_ids, merged_tree)
            .write()
            .await?;

        let before_commit = ws
            .resolve_change_id(&self.before_id)
            .context("resolve before_id")?;
        if ws.check_immutable(vec![before_commit.id().clone()])? && !options.ignore_immutable {
            precondition!("'Before' revision is immutable");
        }

        rewrite::rebase_commit(tx.repo_mut(), before_commit, vec![new_commit.id().clone()]).await?;

        tx.repo_mut()
            .edit(ws.name().to_owned(), &new_commit)
            .await?;

        match ws.finish_transaction(tx, "new empty commit").await? {
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

        commit_builder.write().await?;

        match ws
            .finish_transaction(tx, format!("describe commit {}", described.id().hex()))
            .await?
        {
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
            let clone_parents: Vec<_> = clonee
                .parents()
                .await?
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
            let clone = tx
                .repo_mut()
                .rewrite_commit(&clonee)
                .clear_rewrite_source()
                .generate_new_change_id()
                .set_parents(clone_parents)
                .write()
                .await?;
            clones.insert(clonee, clone);
        }

        match ws
            .finish_transaction(tx, format!("duplicating {} commit(s)", num_clonees))
            .await?
        {
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
        let rebased_children = disinherit_children(ws, &mut tx, &targets, orphan_to).await?;

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
            tx.repo_mut()
                .rebase_descendants_with_options(
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
                )
                .await?;
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

        match ws.finish_transaction(tx, transaction_description).await? {
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
        let rebased_children = disinherit_children(ws, &mut tx, &targets, orphan_to).await?;

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

        match ws.finish_transaction(tx, transaction_description).await? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{
        messages::{RevSet, queries::RevsResult},
        worker::{
            WorkerSession, queries,
            tests::{get_by_chid, mkrepo, query_by_id, revs},
        },
    };
    use anyhow::Result;
    use assert_matches::assert_matches;
    use jj_lib::{
        config::{ConfigLayer, ConfigSource},
        repo::Repo,
        settings::UserSettings,
    };
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn abandon_revisions() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let page = queries::query_log(&ws, "all()", 100)?;
        assert_eq!(24, page.rows.len());

        AbandonRevisions {
            set: RevSet::singleton(revs::resolve_conflict()),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let page = queries::query_log(&ws, "all()", 100)?;
        assert_eq!(23, page.rows.len());

        Ok(())
    }

    #[tokio::test]
    async fn abandon_revisions_range() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let abandoned_set = RevSet::sequence(revs::conflict_bookmark(), revs::resolve_conflict());

        let before_page = queries::query_log(&ws, "all()", 100)?;
        let before_count = before_page.rows.len();

        let before_result = queries::query_revisions(&ws, abandoned_set.clone()).await?;
        assert_matches!(
            before_result,
            RevsResult::Detail { .. },
            "Querying range before abandon should return Detail"
        );

        // abandon hides the revisions from both log paging and direct queries
        AbandonRevisions {
            set: abandoned_set.clone(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let after_page = queries::query_log(&ws, "all()", 100)?;
        assert_eq!(before_count - 2, after_page.rows.len());

        let after_result = queries::query_revisions(&ws, abandoned_set).await?;
        assert_matches!(
            after_result,
            RevsResult::NotFound { .. },
            "Querying abandoned range should return NotFound"
        );

        Ok(())
    }

    #[tokio::test]
    async fn adopt_revision() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let page = queries::query_log(&ws, "@+", 1)?;
        assert_eq!(0, page.rows.len());

        AdoptRevision {
            id: revs::resolve_conflict(),
            parent_ids: vec![revs::working_copy().commit],
        }
        .execute_unboxed(&mut ws)
        .await?;

        let page = queries::query_log(&ws, "@+", 2)?;
        assert_eq!(1, page.rows.len());

        Ok(())
    }

    #[tokio::test]
    async fn backout_revisions() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // update to small_child, which contains small.txt with "line1\nchanged\n"
        CheckoutRevision {
            id: revs::small_child(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        // revert small_child, which changed line2 from "line2" to "changed"
        let result = BackoutRevisions {
            set: RevSet::singleton(revs::small_child()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify small.txt is no longer "changed"
        let wc = ws.get_commit(ws.wc_id())?;
        let tree = wc.tree();
        let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("small.txt")?;

        match tree.path_value(&repo_path)?.into_resolved() {
            Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
                let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
                let mut content = Vec::new();
                reader.read_to_end(&mut content).await?;
                let content_str = String::from_utf8_lossy(&content);
                assert_eq!(
                    content_str, "line1\nline2\n",
                    "backout should revert line2 from 'changed' back to 'line2'"
                );
            }
            _ => panic!("expected small.txt to be a resolved file"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn backout_revisions_range() -> Result<()> {
        use jj_lib::repo::Repo;

        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // update to hunk_grandchild
        CheckoutRevision {
            id: revs::hunk_grandchild(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        // revert hunk_child_single::hunk_grandchild, which reverses both line2 -> modified2 and line3 -> grandchild3
        let result = BackoutRevisions {
            set: RevSet::sequence(revs::hunk_child_single(), revs::hunk_grandchild()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify hunk_test.txt content: should be back to hunk_base state
        let wc = ws.get_commit(ws.wc_id())?;
        let tree = wc.tree();
        let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

        match tree.path_value(&repo_path)?.into_resolved() {
            Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
                let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
                let mut content = Vec::new();
                reader.read_to_end(&mut content).await?;
                let content_str = String::from_utf8_lossy(&content);
                assert_eq!(
                    content_str, "line1\nline2\nline3\nline4\nline5\n",
                    "backout of range should return to hunk_base content"
                );
            }
            _ => panic!("expected hunk_test.txt to be a resolved file"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn checkout_revision() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let head_header = queries::query_revision(&ws, &revs::working_copy())?;
        let conflict_header = queries::query_revision(&ws, &revs::conflict_bookmark())?;
        assert!(head_header.expect("exists").is_working_copy);
        assert!(!conflict_header.expect("exists").is_working_copy);

        let result = CheckoutRevision {
            id: revs::conflict_bookmark(),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        let head_header = queries::query_revision(&ws, &revs::working_copy())?;
        let conflict_header = queries::query_revision(&ws, &revs::conflict_bookmark())?;
        assert!(
            head_header.is_none(),
            "old working copy revision should not exist"
        );
        assert!(conflict_header.expect("exists").is_working_copy);

        Ok(())
    }

    #[tokio::test]
    async fn create_revision_single_parent() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let parent_header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
        assert!(parent_header.is_working_copy);

        let result = CreateRevision {
            set: RevSet::singleton(revs::working_copy()),
        }
        .execute_unboxed(&mut ws)
        .await?;

        match result {
            MutationResult::Updated {
                new_selection: Some(new_selection),
                ..
            } => {
                let parent_header =
                    queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
                let child_header =
                    queries::query_revision(&ws, &new_selection.id)?.expect("exists");
                assert!(!parent_header.is_working_copy);
                assert!(child_header.is_working_copy);
            }
            _ => panic!("CreateRevision failed"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn create_revision_multi_parent() -> Result<()> {
        let repo: tempfile::TempDir = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // conflict_bookmark is parent of resolve_conflict, forming a linear range of 2 commits
        let result = CreateRevision {
            set: RevSet::sequence(revs::conflict_bookmark(), revs::resolve_conflict()),
        }
        .execute_unboxed(&mut ws)
        .await?;

        match result {
            MutationResult::Updated {
                new_selection: Some(new_selection),
                ..
            } => {
                let child_rev = query_by_id(&ws, new_selection.id).await?;
                assert_matches!(child_rev, RevsResult::Detail { parents, .. } if parents.len() == 2);
            }
            _ => panic!("CreateRevision failed"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn describe_revision() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
        assert!(header.description.lines[0].is_empty());

        let result = DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        let header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
        assert_eq!(header.description.lines[0], "wip");

        Ok(())
    }

    #[tokio::test]
    async fn describe_revision_with_snapshot() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let rev = query_by_id(&ws, revs::working_copy()).await?;
        assert_matches!(rev, RevsResult::Detail { headers, changes, .. } if headers.last().unwrap().description.lines[0].is_empty() && changes.is_empty());

        fs::write(repo.path().join("new.txt"), []).unwrap(); // changes the WC commit

        DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;

        let rev = query_by_id(&ws, revs::working_copy()).await?;
        assert_matches!(rev, RevsResult::Detail { headers, changes, .. } if headers.last().unwrap().description.lines[0] == "wip" && !changes.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn describe_revision_reset_author_rejects_empty_name() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let mut config = ws.data.workspace_settings.config().clone();
        config.add_layer(
            ConfigLayer::parse(
                ConfigSource::CommandArg,
                "user.name = \"\"\nuser.email = \"test@example.com\"",
            )
            .unwrap(),
        );
        ws.data.workspace_settings = UserSettings::from_config(config).unwrap();

        let result = DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: true,
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("Name not configured"));

        Ok(())
    }

    #[tokio::test]
    async fn describe_revision_reset_author_rejects_empty_email() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let mut config = ws.data.workspace_settings.config().clone();
        config.add_layer(
            ConfigLayer::parse(
                ConfigSource::CommandArg,
                "user.name = \"Test User\"\nuser.email = \"\"",
            )
            .unwrap(),
        );
        ws.data.workspace_settings = UserSettings::from_config(config).unwrap();

        let result = DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: true,
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("Email not configured"));

        Ok(())
    }

    #[tokio::test]
    async fn describe_revision_reset_author_rejects_empty_name_and_email() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let mut config = ws.data.workspace_settings.config().clone();
        config.add_layer(
            ConfigLayer::parse(
                ConfigSource::CommandArg,
                "user.name = \"\"\nuser.email = \"\"",
            )
            .unwrap(),
        );
        ws.data.workspace_settings = UserSettings::from_config(config).unwrap();

        let result = DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: true,
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("Name and email not configured"));

        Ok(())
    }

    #[tokio::test]
    async fn duplicate_revisions() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
        assert!(header.description.lines[0].is_empty());

        let result = DuplicateRevisions {
            set: RevSet::singleton(revs::main_bookmark()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        let page = queries::query_log(&ws, "description(unsynced)", 3)?;
        assert_eq!(2, page.rows.len());

        Ok(())
    }

    #[tokio::test]
    async fn duplicate_revisions_range() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let page = queries::query_log(&ws, "all()", 100)?;
        let initial_count = page.rows.len();

        let result = DuplicateRevisions {
            set: RevSet::sequence(revs::conflict_bookmark(), revs::resolve_conflict()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        let page = queries::query_log(&ws, "all()", 100)?;
        assert_eq!(initial_count + 2, page.rows.len());

        Ok(())
    }

    #[tokio::test]
    async fn insert_revisions_single() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let page = queries::query_log(&ws, "main::@", 4)?;
        assert_eq!(2, page.rows.len());

        InsertRevisions {
            set: RevSet::singleton(revs::resolve_conflict()),
            after_id: revs::main_bookmark(),
            before_id: revs::working_copy(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let page = queries::query_log(&ws, "main::@", 4)?;
        assert_eq!(3, page.rows.len());

        Ok(())
    }

    #[tokio::test]
    async fn insert_revisions_range() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // before: main -> @
        let page = queries::query_log(&ws, "main::@", 5)?;
        assert_eq!(2, page.rows.len());

        // insert the range conflict_bookmark -> resolve_conflict between main and @
        let result = InsertRevisions {
            set: RevSet::sequence(revs::conflict_bookmark(), revs::resolve_conflict()),
            after_id: revs::main_bookmark(),
            before_id: revs::working_copy(),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // after: main -> conflict_bookmark -> resolve_conflict -> @
        let page = queries::query_log(&ws, "main::@", 5)?;
        assert_eq!(4, page.rows.len());

        // verify structure: conflict_bookmark is now child of main
        let conflict_after = get_by_chid(&ws, &revs::conflict_bookmark())?;
        let conflict_parents: Vec<_> = conflict_after.parent_ids().to_vec();
        assert_eq!(conflict_parents.len(), 1);
        assert_eq!(conflict_parents[0].hex(), revs::main_bookmark().commit.hex);

        // verify resolve_conflict is still child of conflict_bookmark (internal structure preserved)
        let resolve_after = get_by_chid(&ws, &revs::resolve_conflict())?;
        let resolve_parents: Vec<_> = resolve_after.parent_ids().to_vec();
        assert_eq!(resolve_parents.len(), 1);
        assert_eq!(resolve_parents[0], conflict_after.id().clone());

        // verify @ is now child of resolve_conflict
        let wc_after = ws.get_commit(ws.wc_id())?;
        let wc_parents: Vec<_> = wc_after.parent_ids().to_vec();
        assert_eq!(wc_parents.len(), 1);
        assert_eq!(wc_parents[0], resolve_after.id().clone());

        Ok(())
    }

    #[tokio::test]
    async fn insert_revisions_single_preserves_merge_child_other_parents() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let merge_before = get_by_chid(&ws, &revs::chain_conflict())?;
        let merge_parents_before: Vec<_> = merge_before.parent_ids().to_vec();
        assert!(
            merge_parents_before.len() >= 2,
            "expected merge commit in test repository"
        );

        let replaced_parent_id = merge_parents_before[0].clone();
        let preserved_parent_ids = merge_parents_before[1..].to_vec();
        let after_id = ws.format_id(&ws.get_commit(&replaced_parent_id)?);

        let result = InsertRevisions {
            set: RevSet::singleton(revs::hunk_grandchild()),
            after_id,
            before_id: revs::chain_conflict(),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        let inserted_after = get_by_chid(&ws, &revs::hunk_grandchild())?;
        let merge_after = get_by_chid(&ws, &revs::chain_conflict())?;
        let merge_parents_after: Vec<_> = merge_after.parent_ids().to_vec();

        assert_eq!(merge_parents_after.len(), merge_parents_before.len());
        assert!(
            merge_parents_after.contains(inserted_after.id()),
            "merge child should include inserted revision as a parent"
        );
        assert!(
            !merge_parents_after.contains(&replaced_parent_id),
            "targeted parent edge should be replaced"
        );

        for preserved_parent_id in preserved_parent_ids {
            assert!(
                merge_parents_after.contains(&preserved_parent_id),
                "merge child should keep non-target parents"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn move_revisions_single() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // initially, resolve_conflict is a child of conflict_bookmark
        let before = get_by_chid(&ws, &revs::resolve_conflict())?;
        let before_parents: Vec<_> = before.parent_ids().to_vec();
        assert_eq!(before_parents.len(), 1);
        assert_eq!(
            before_parents[0].hex(),
            revs::conflict_bookmark().commit.hex
        );

        MoveRevisions {
            set: RevSet::singleton(revs::resolve_conflict()),
            parent_ids: vec![revs::main_bookmark()],
        }
        .execute_unboxed(&mut ws)
        .await?;

        // verify it's now a child of main_bookmark
        let after = get_by_chid(&ws, &revs::resolve_conflict())?;
        let after_parents: Vec<_> = after.parent_ids().to_vec();
        assert_eq!(after_parents.len(), 1);
        assert_eq!(after_parents[0].hex(), revs::main_bookmark().commit.hex);

        Ok(())
    }

    #[tokio::test]
    async fn move_revisions_range() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // rebase conflict_bookmark::resolve_conflict onto the working copy
        let result = MoveRevisions {
            set: RevSet::sequence(revs::conflict_bookmark(), revs::resolve_conflict()),
            parent_ids: vec![revs::working_copy()],
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify conflict_bookmark is now a child of working_copy (the oldest in the range was rebased)
        let after = get_by_chid(&ws, &revs::conflict_bookmark())?;
        let after_parents: Vec<_> = after.parent_ids().to_vec();
        assert_eq!(after_parents.len(), 1);
        assert_eq!(after_parents[0].hex(), revs::working_copy().commit.hex);

        // verify resolve_conflict is still a child of conflict_bookmark (internal structure preserved)
        let resolve_after = get_by_chid(&ws, &revs::resolve_conflict())?;
        let resolve_parents: Vec<_> = resolve_after.parent_ids().to_vec();
        assert_eq!(resolve_parents.len(), 1);
        assert_eq!(resolve_parents[0], after.id().clone());

        Ok(())
    }

    /// Test moving a range disinherits external children of the newest commit.
    ///
    /// Setup: hunk_child_single -> hunk_grandchild (the range to move)
    /// hunk_child_single is child of hunk_base
    /// Move hunk_child_single::hunk_grandchild to main_bookmark
    /// Verify: hunk_grandchild moves with the range (it's internal to the range)
    #[tokio::test]
    async fn move_revisions_range_internal_structure_preserved() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // get original parent of hunk_child_single (should be hunk_base)
        let child_before = get_by_chid(&ws, &revs::hunk_child_single())?;
        let child_parents_before: Vec<_> = child_before.parent_ids().to_vec();
        assert_eq!(child_parents_before.len(), 1);
        assert_eq!(
            child_parents_before[0].hex(),
            revs::hunk_base().commit.hex,
            "hunk_child_single should be child of hunk_base before move"
        );

        // move hunk_child_single::hunk_grandchild onto main_bookmark
        let result = MoveRevisions {
            set: RevSet::sequence(revs::hunk_child_single(), revs::hunk_grandchild()),
            parent_ids: vec![revs::main_bookmark()],
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify hunk_child_single is now a child of main_bookmark
        let child_after = get_by_chid(&ws, &revs::hunk_child_single())?;
        let child_parents_after: Vec<_> = child_after.parent_ids().to_vec();
        assert_eq!(child_parents_after.len(), 1);
        assert_eq!(
            child_parents_after[0].hex(),
            revs::main_bookmark().commit.hex,
            "hunk_child_single should be child of main_bookmark after move"
        );

        // verify hunk_grandchild is still a child of hunk_child_single (internal structure preserved)
        let grandchild_after = get_by_chid(&ws, &revs::hunk_grandchild())?;
        let grandchild_parents: Vec<_> = grandchild_after.parent_ids().to_vec();
        assert_eq!(grandchild_parents.len(), 1);
        assert_eq!(
            grandchild_parents[0],
            child_after.id().clone(),
            "hunk_grandchild should still be child of hunk_child_single"
        );

        Ok(())
    }

    /// Test that moving a range disinherits external children to the oldest commit's parent.
    ///
    /// Setup: Create A -> B -> C -> D where we move B::C
    /// D is an external child of C (newest in range)
    /// After move, D should be orphaned to A (B's original parent), not to B
    #[tokio::test]
    async fn move_revisions_range_disinherits_to_oldest_parent() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // Create a chain: working_copy -> A -> B -> C
        // Then move A::B somewhere else, C should be orphaned to working_copy (A's parent)

        // First, create commit A on top of working_copy
        let result = CreateRevision {
            set: RevSet::singleton(revs::working_copy()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let a_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };

        fs::write(repo.path().join("chain_a.txt"), "commit A").unwrap();
        DescribeRevision {
            id: a_id.clone(),
            new_description: "commit A".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let a = get_by_chid(&ws, &a_id)?;
        let a_id = ws.format_id(&a);

        // Create commit B on top of A
        let result = CreateRevision {
            set: RevSet::singleton(a_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let b_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };

        fs::write(repo.path().join("chain_b.txt"), "commit B").unwrap();
        DescribeRevision {
            id: b_id.clone(),
            new_description: "commit B".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let b = get_by_chid(&ws, &b_id)?;
        let b_id = ws.format_id(&b);

        // Create commit C on top of B (this will be the external child after moving A::B)
        let result = CreateRevision {
            set: RevSet::singleton(b_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let c_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };

        fs::write(repo.path().join("chain_c.txt"), "commit C").unwrap();
        DescribeRevision {
            id: c_id.clone(),
            new_description: "commit C".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let c = get_by_chid(&ws, &c_id)?;
        let c_id = ws.format_id(&c);

        // verify C is child of B before the move
        let c_parents_before: Vec<_> = c.parent_ids().to_vec();
        assert_eq!(c_parents_before.len(), 1);

        // Move A::B to main_bookmark
        // C should be orphaned to working_copy (A's original parent), not to A
        let result = MoveRevisions {
            set: RevSet::sequence(a_id.clone(), b_id.clone()),
            parent_ids: vec![revs::main_bookmark()],
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify A is now child of main_bookmark
        let a_after = get_by_chid(&ws, &a_id)?;
        let a_parents_after: Vec<_> = a_after.parent_ids().to_vec();
        assert_eq!(
            a_parents_after[0].hex(),
            revs::main_bookmark().commit.hex,
            "A should be child of main_bookmark after move"
        );

        // verify B is still child of A (internal structure preserved)
        let b_after = get_by_chid(&ws, &b_id)?;
        let b_parents_after: Vec<_> = b_after.parent_ids().to_vec();
        assert_eq!(
            b_parents_after[0],
            a_after.id().clone(),
            "B should still be child of A"
        );

        // CRITICAL: verify C is now child of working_copy (A's original parent)
        // NOT child of B (which would mean it moved with the range)
        let c_after = get_by_chid(&ws, &c_id)?;
        let c_parents_after: Vec<_> = c_after.parent_ids().to_vec();
        assert_eq!(c_parents_after.len(), 1);
        assert_eq!(
            c_parents_after[0].hex(),
            revs::working_copy().commit.hex,
            "C should be orphaned to working_copy (A's original parent), not follow the moved range"
        );

        Ok(())
    }

    /// Test that moving a range disinherits external children of middle commits too.
    ///
    /// Setup: Create A -> B -> C where B has a sibling child D
    ///        A
    ///        |
    ///        B -- D
    ///        |
    ///        C
    /// Move A::C somewhere else. D should be orphaned to the parent of A.
    #[tokio::test]
    async fn move_revisions_range_disinherits_children_of_middle() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // Create commit A on working_copy
        let result = CreateRevision {
            set: RevSet::singleton(revs::working_copy()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let a_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("middle_a.txt"), "commit A").unwrap();
        DescribeRevision {
            id: a_id.clone(),
            new_description: "commit A".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let a = get_by_chid(&ws, &a_id)?;
        let a_id = ws.format_id(&a);

        // Create commit B on A
        let result = CreateRevision {
            set: RevSet::singleton(a_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let b_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("middle_b.txt"), "commit B").unwrap();
        DescribeRevision {
            id: b_id.clone(),
            new_description: "commit B".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let b = get_by_chid(&ws, &b_id)?;
        let b_id = ws.format_id(&b);

        // Create commit C on B (end of the range we'll move)
        let result = CreateRevision {
            set: RevSet::singleton(b_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let c_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("middle_c.txt"), "commit C").unwrap();
        DescribeRevision {
            id: c_id.clone(),
            new_description: "commit C".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let c = get_by_chid(&ws, &c_id)?;
        let c_id = ws.format_id(&c);

        // Create commit D as another child of B (sibling of C, external to range A::C)
        // First checkout B to create D there
        CheckoutRevision { id: b_id.clone() }
            .execute_unboxed(&mut ws)
            .await?;

        let result = CreateRevision {
            set: RevSet::singleton(b_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let d_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("middle_d.txt"), "commit D").unwrap();
        DescribeRevision {
            id: d_id.clone(),
            new_description: "commit D (sibling of C, child of B)".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let d = get_by_chid(&ws, &d_id)?;
        let d_id = ws.format_id(&d);

        // verify D is child of B before the move
        let d_parents_before: Vec<_> = d.parent_ids().to_vec();
        assert_eq!(d_parents_before.len(), 1);
        assert_eq!(
            d_parents_before[0],
            b.id().clone(),
            "D should be child of B before move"
        );

        // Move A::C to main_bookmark
        // D is child of B (middle of range), should be orphaned to working_copy (A's original parent)
        let result = MoveRevisions {
            set: RevSet::sequence(a_id.clone(), c_id.clone()),
            parent_ids: vec![revs::main_bookmark()],
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify A moved to main_bookmark
        let a_after = get_by_chid(&ws, &a_id)?;
        let a_parents_after: Vec<_> = a_after.parent_ids().to_vec();
        assert_eq!(
            a_parents_after[0].hex(),
            revs::main_bookmark().commit.hex,
            "A should be child of main_bookmark"
        );

        // verify internal structure: B child of A, C child of B
        let b_after = get_by_chid(&ws, &b_id)?;
        assert_eq!(b_after.parent_ids()[0], a_after.id().clone());
        let c_after = get_by_chid(&ws, &c_id)?;
        assert_eq!(c_after.parent_ids()[0], b_after.id().clone());

        // CRITICAL: D should be orphaned to working_copy, not follow B
        let d_after = get_by_chid(&ws, &d_id)?;
        let d_parents_after: Vec<_> = d_after.parent_ids().to_vec();
        assert_eq!(d_parents_after.len(), 1);
        assert_eq!(
            d_parents_after[0].hex(),
            revs::working_copy().commit.hex,
            "D should be orphaned to working_copy (A's original parent), not follow the moved range"
        );

        Ok(())
    }

    /// Test that moving a range with multiple external children handles all of them.
    ///
    /// Setup: A has children B and C. B has child D (end of range).
    ///        Move A::D. C should be orphaned.
    #[tokio::test]
    async fn move_revisions_range_multiple_external_children() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // Create commit A
        let result = CreateRevision {
            set: RevSet::singleton(revs::working_copy()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let a_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("multi_a.txt"), "commit A").unwrap();
        DescribeRevision {
            id: a_id.clone(),
            new_description: "commit A".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let a = get_by_chid(&ws, &a_id)?;
        let a_id = ws.format_id(&a);

        // Create commit B on A (part of the range)
        let result = CreateRevision {
            set: RevSet::singleton(a_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let b_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("multi_b.txt"), "commit B").unwrap();
        DescribeRevision {
            id: b_id.clone(),
            new_description: "commit B".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let b = get_by_chid(&ws, &b_id)?;
        let b_id = ws.format_id(&b);

        // Create commit C on A (sibling of B, external to range)
        CheckoutRevision { id: a_id.clone() }
            .execute_unboxed(&mut ws)
            .await?;

        let result = CreateRevision {
            set: RevSet::singleton(a_id.clone()),
        }
        .execute_unboxed(&mut ws)
        .await?;
        let c_id = match result {
            MutationResult::Updated {
                new_selection: Some(sel),
                ..
            } => sel.id,
            _ => panic!("expected new revision"),
        };
        fs::write(repo.path().join("multi_c.txt"), "commit C").unwrap();
        DescribeRevision {
            id: c_id.clone(),
            new_description: "commit C (sibling of B)".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;
        let c = get_by_chid(&ws, &c_id)?;
        let c_id = ws.format_id(&c);

        // verify C is child of A
        assert_eq!(c.parent_ids()[0], a.id().clone());

        // Move A::B to main_bookmark
        // C is external child of A (oldest in range), should be orphaned to working_copy
        let result = MoveRevisions {
            set: RevSet::sequence(a_id.clone(), b_id.clone()),
            parent_ids: vec![revs::main_bookmark()],
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // verify A moved
        let a_after = get_by_chid(&ws, &a_id)?;
        assert_eq!(
            a_after.parent_ids()[0].hex(),
            revs::main_bookmark().commit.hex
        );

        // verify B still child of A
        let b_after = get_by_chid(&ws, &b_id)?;
        assert_eq!(b_after.parent_ids()[0], a_after.id().clone());

        // CRITICAL: C should be orphaned to working_copy
        let c_after = get_by_chid(&ws, &c_id)?;
        assert_eq!(
            c_after.parent_ids()[0].hex(),
            revs::working_copy().commit.hex,
            "C should be orphaned to working_copy (A's original parent)"
        );

        Ok(())
    }
}

/// Disinherit external children of a range of commits.
/// Finds all children of any commit in the range that are not themselves in the range,
/// and rebases them to the specified orphan parents (typically the oldest commit's parents).
async fn disinherit_children(
    ws: &WorkspaceSession<'_>,
    tx: &mut Transaction,
    range: &[Commit],
    orphan_to: Vec<CommitId>,
) -> Result<HashMap<CommitId, CommitId>> {
    let range_ids: HashSet<CommitId> = range.iter().map(|c| c.id().clone()).collect();

    // find all children of any commit in the range
    let mut external_children: Vec<Commit> = Vec::new();
    for target in range {
        let children_expr = RevsetExpression::commit(target.id().clone()).children();
        let children: Vec<Commit> = children_expr
            .evaluate(ws.repo())?
            .iter()
            .commits(ws.repo().store())
            .try_collect()?;

        for child in children {
            if !range_ids.contains(child.id()) {
                external_children.push(child);
            }
        }
    }

    // dedupe in case a child has multiple parents in the range
    external_children.sort_by_key(|c| c.id().clone());
    external_children.dedup_by_key(|c| c.id().clone());

    if external_children.is_empty() {
        return Ok(HashMap::new());
    }

    // rebase each external child, replacing any parent in the range with orphan_to
    let mut rebased_commit_ids = HashMap::new();
    for child_commit in external_children {
        let new_child_parent_ids: Vec<CommitId> = child_commit
            .parent_ids()
            .iter()
            .flat_map(|c| {
                if range_ids.contains(c) {
                    orphan_to.clone()
                } else {
                    vec![c.clone()]
                }
            })
            .collect_vec();

        // some of the new parents may be ancestors of others
        let new_child_parents_expression = RevsetExpression::commits(new_child_parent_ids.clone())
            .minus(
                &RevsetExpression::commits(new_child_parent_ids.clone())
                    .parents()
                    .ancestors(),
            );
        let new_child_parents: Result<Vec<CommitId>, _> = new_child_parents_expression
            .evaluate(tx.base_repo().as_ref())?
            .iter()
            .collect();

        rebased_commit_ids.insert(
            child_commit.id().clone(),
            rewrite::rebase_commit(tx.repo_mut(), child_commit, new_child_parents?)
                .await?
                .id()
                .clone(),
        );
    }

    // rebase descendants of modified commits, tracking new ids
    let mut mapping = HashMap::new();
    tx.repo_mut()
        .rebase_descendants_with_options(&RebaseOptions::default(), |old_commit, rebased| {
            mapping.insert(
                old_commit.id().clone(),
                match rebased {
                    RebasedCommit::Rewritten(new_commit) => new_commit.id().clone(),
                    RebasedCommit::Abandoned { parent_id } => parent_id,
                },
            );
        })
        .await?;
    rebased_commit_ids.extend(mapping);

    Ok(rebased_commit_ids)
}
