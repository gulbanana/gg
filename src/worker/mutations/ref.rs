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
