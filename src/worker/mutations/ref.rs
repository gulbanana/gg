use std::fmt::Display;

use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use jj_lib::{
    git::REMOTE_NAME_FOR_LOCAL_GIT_REPO,
    object_id::ObjectId as ObjectIdTrait,
    op_store::{RefTarget, RemoteRef, RemoteRefState},
    ref_name::{RefNameBuf, RemoteNameBuf, RemoteRefSymbol},
    str_util::StringPattern,
};

use super::precondition;

use crate::{
    messages::{
        StoreRef,
        mutations::{
            CreateRef, DeleteRef, MoveRef, MutationOptions, MutationResult, RenameBookmark,
            TrackBookmark, UntrackBookmark,
        },
    },
    worker::{Mutation, gui_util::WorkspaceSession},
};

#[async_trait(?Send)]
impl Mutation for TrackBookmark {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
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

fn combine_bookmarks(bookmark_names: &[impl Display]) -> String {
    match bookmark_names {
        [bookmark_name] => format!("bookmark {}", bookmark_name),
        bookmark_names => format!("bookmarks {}", bookmark_names.iter().join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use assert_matches::assert_matches;
    use jj_lib::str_util::StringMatcher;

    use crate::{
        messages::{
            StoreRef,
            mutations::{
                CreateRef, DeleteRef, MoveRef, MutationResult, RenameBookmark, TrackBookmark,
                UntrackBookmark,
            },
        },
        worker::{
            Mutation, WorkerSession,
            tests::{mkrepo, revs},
        },
    };

    fn local_bookmark(name: &str) -> StoreRef {
        StoreRef::LocalBookmark {
            bookmark_name: name.to_owned(),
            has_conflict: false,
            is_synced: false,
            tracking_remotes: vec![],
            available_remotes: 0,
            potential_remotes: 0,
        }
    }

    fn remote_bookmark(name: &str, remote: &str) -> StoreRef {
        StoreRef::RemoteBookmark {
            bookmark_name: name.to_owned(),
            remote_name: remote.to_owned(),
            has_conflict: false,
            is_synced: false,
            is_tracked: false,
            is_absent: false,
        }
    }

    fn tag(name: &str) -> StoreRef {
        StoreRef::Tag {
            tag_name: name.to_owned(),
        }
    }

    // -- CreateRef --

    #[tokio::test]
    async fn create_bookmark() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = CreateRef {
            id: revs::main_bookmark(),
            r#ref: local_bookmark("new-bookmark"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let matcher = StringMatcher::Exact("new-bookmark".to_string());
        let (_, target) = ws
            .view()
            .local_bookmarks_matching(&matcher)
            .next()
            .expect("bookmark should exist");
        assert!(target.is_present());

        Ok(())
    }

    #[tokio::test]
    async fn create_bookmark_already_exists() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = CreateRef {
            id: revs::working_copy(),
            r#ref: local_bookmark("main"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn create_remote_bookmark_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = CreateRef {
            id: revs::working_copy(),
            r#ref: remote_bookmark("main", "origin"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn create_tag() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = CreateRef {
            id: revs::main_bookmark(),
            r#ref: tag("v1.0"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let matcher = StringMatcher::Exact("v1.0".to_string());
        let (_, target) = ws
            .view()
            .local_tags_matching(&matcher)
            .next()
            .expect("tag should exist");
        assert!(target.is_present());

        Ok(())
    }

    #[tokio::test]
    async fn create_tag_already_exists() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        // create tag first
        CreateRef {
            id: revs::main_bookmark(),
            r#ref: tag("v1.0"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        // creating the same tag again should fail
        let result = CreateRef {
            id: revs::working_copy(),
            r#ref: tag("v1.0"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    // -- DeleteRef --

    #[tokio::test]
    async fn delete_bookmark() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = DeleteRef {
            r#ref: local_bookmark("main"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let matcher = StringMatcher::Exact("main".to_string());
        assert!(ws.view().local_bookmarks_matching(&matcher).next().is_none());

        Ok(())
    }

    #[tokio::test]
    async fn delete_tag() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        // create then delete
        CreateRef {
            id: revs::main_bookmark(),
            r#ref: tag("ephemeral"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let result = DeleteRef {
            r#ref: tag("ephemeral"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let matcher = StringMatcher::Exact("ephemeral".to_string());
        assert!(ws.view().local_tags_matching(&matcher).next().is_none());

        Ok(())
    }

    // -- MoveRef --

    #[tokio::test]
    async fn move_bookmark() -> Result<()> {
        use jj_lib::object_id::ObjectId;

        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = MoveRef {
            r#ref: local_bookmark("main"),
            to_id: revs::working_copy(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let matcher = StringMatcher::Exact("main".to_string());
        let (_, target) = ws
            .view()
            .local_bookmarks_matching(&matcher)
            .next()
            .expect("bookmark should still exist");
        let commit_id = target.as_normal().expect("should be a normal ref");
        assert_eq!(commit_id.hex(), revs::working_copy().commit.hex);

        Ok(())
    }

    #[tokio::test]
    async fn move_nonexistent_bookmark() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = MoveRef {
            r#ref: local_bookmark("does-not-exist"),
            to_id: revs::working_copy(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn move_remote_bookmark_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = MoveRef {
            r#ref: remote_bookmark("main", "origin"),
            to_id: revs::working_copy(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn move_tag() -> Result<()> {
        use jj_lib::object_id::ObjectId;

        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        CreateRef {
            id: revs::main_bookmark(),
            r#ref: tag("v2.0"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let result = MoveRef {
            r#ref: tag("v2.0"),
            to_id: revs::working_copy(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let matcher = StringMatcher::Exact("v2.0".to_string());
        let (_, target) = ws
            .view()
            .local_tags_matching(&matcher)
            .next()
            .expect("tag should still exist");
        let commit_id = target.as_normal().expect("should be a normal ref");
        assert_eq!(commit_id.hex(), revs::working_copy().commit.hex);

        Ok(())
    }

    // -- RenameBookmark --

    #[tokio::test]
    async fn rename_bookmark() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = RenameBookmark {
            r#ref: local_bookmark("main"),
            new_name: "trunk".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });

        let old_matcher = StringMatcher::Exact("main".to_string());
        assert!(ws.view().local_bookmarks_matching(&old_matcher).next().is_none());

        let new_matcher = StringMatcher::Exact("trunk".to_string());
        assert!(ws.view().local_bookmarks_matching(&new_matcher).next().is_some());

        Ok(())
    }

    #[tokio::test]
    async fn rename_nonexistent_bookmark() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = RenameBookmark {
            r#ref: local_bookmark("ghost"),
            new_name: "something".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn rename_bookmark_to_existing_name() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        // create a second bookmark so we can try renaming to it
        CreateRef {
            id: revs::working_copy(),
            r#ref: local_bookmark("other"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let result = RenameBookmark {
            r#ref: local_bookmark("main"),
            new_name: "other".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    // -- TrackBookmark --

    #[tokio::test]
    async fn track_tag_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = TrackBookmark {
            r#ref: tag("v1.0"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn track_local_bookmark_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = TrackBookmark {
            r#ref: local_bookmark("main"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    // -- UntrackBookmark --

    #[tokio::test]
    async fn untrack_tag_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let result = UntrackBookmark {
            r#ref: tag("v1.0"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    // -- TrackBookmark + UntrackBookmark round-trip --

    #[tokio::test]
    async fn track_already_tracked_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        // main@origin is tracked by default in the test repo
        let result = TrackBookmark {
            r#ref: remote_bookmark("main", "origin"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    #[tokio::test]
    async fn untrack_then_track_round_trip() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        // untrack main@origin
        let result = UntrackBookmark {
            r#ref: remote_bookmark("main", "origin"),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        // track it again
        let result = TrackBookmark {
            r#ref: remote_bookmark("main", "origin"),
        }
        .execute_unboxed(&mut ws)
        .await?;
        assert_matches!(result, MutationResult::Updated { .. });

        Ok(())
    }

    #[tokio::test]
    async fn untrack_not_tracked_fails() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        // untrack main@origin first
        UntrackBookmark {
            r#ref: remote_bookmark("main", "origin"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        // untracking again should fail
        let result = UntrackBookmark {
            r#ref: remote_bookmark("main", "origin"),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { .. });

        Ok(())
    }

    // -- combine_bookmarks --

    #[test]
    fn combine_single_bookmark() {
        assert_eq!(super::combine_bookmarks(&["main"]), "bookmark main");
    }

    #[test]
    fn combine_multiple_bookmarks() {
        assert_eq!(
            super::combine_bookmarks(&["main", "dev"]),
            "bookmarks main, dev"
        );
    }
}
