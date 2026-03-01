//! Serializable message types shared between the Rust backend and the
//! TypeScript frontend. Not currently extensible.
//!
//! Every type here derives [`Serialize`] and/or
//! [`Deserialize`]. When the `ts-rs` feature is enabled,
//! `cargo gen` (alias for `cargo test -F ts-rs`) exports matching TypeScript
//! definitions into `app/messages/`.
//!
//! The most important subsets are:
//!
//! - **Queries** ([`queries`] submodule) — request/response types for read-only
//!   operations like log listing and revision details.
//! - **Mutations** ([`mutations`] submodule) — request types that modify the
//!   repository. Each mutation struct implements the [`Mutation`](crate::worker::Mutation)
//!   trait, which defines how the change is executed on the worker thread.

mod mutations;
mod queries;

pub use mutations::*;
pub use queries::*;

use std::{collections::HashMap, path::Path};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-rs")]
use ts_rs::TS;

/// Utility type used to abstract crlf/&lt;br&gt;/etc
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MultilineString {
    pub lines: Vec<String>,
}

impl<'a, T> From<T> for MultilineString
where
    T: Into<&'a str>,
{
    fn from(value: T) -> Self {
        MultilineString {
            lines: value.into().split("\n").map(|l| l.to_owned()).collect(),
        }
    }
}

/// Utility type used for platform-specific display
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct DisplayPath(pub String);

impl<T: AsRef<Path>> From<T> for DisplayPath {
    fn from(value: T) -> Self {
        DisplayPath(
            dunce::simplified(value.as_ref())
                .to_string_lossy()
                .to_string(),
        )
    }
}

/// Utility type used for round-tripping
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct TreePath {
    pub repo_path: String,
    pub relative_path: DisplayPath,
}

/// Configuration sent to the frontend when a workspace is opened.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum RepoConfig {
    #[allow(dead_code)] // used by frontend
    Initial,
    Workspace {
        absolute_path: DisplayPath,
        git_remotes: Vec<String>,
        query_choices: HashMap<String, String>,
        latest_query: String,
        status: RepoStatus,
        theme_override: Option<String>,
        mark_unpushed_bookmarks: bool,
        track_recent_workspaces: bool,
        ignore_immutable: bool,
        has_external_diff_tool: bool,
        has_external_merge_tool: bool,
    },
    #[allow(dead_code)] // used by frontend
    TimeoutError,
    LoadError {
        absolute_path: DisplayPath,
        message: String,
    },
    WorkerError {
        message: String,
    },
}

/// Current state of the repository after an operation.
#[derive(Serialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RepoStatus {
    pub operation_description: String,
    pub working_copy: CommitId,
}

/// Events requiring user interaction during clone/init flows.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum RepoEvent {
    CloneURL,
    CloneConfirm { url: String, path: String },
    InitConfirm { path: String, has_git: bool },
}

/// Bookmark or tag name with metadata.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum StoreRef {
    LocalBookmark {
        bookmark_name: String,
        has_conflict: bool,
        /// Synchronized with all tracking remotes
        is_synced: bool,
        /// Actual and potential remotes
        tracking_remotes: Vec<String>,
        available_remotes: usize,
        potential_remotes: usize,
    },
    RemoteBookmark {
        bookmark_name: String,
        remote_name: String,
        has_conflict: bool,
        /// Tracking remote ref is synchronized with local ref
        is_synced: bool,
        /// Has local ref
        is_tracked: bool,
        /// Local ref has been deleted
        is_absent: bool,
    },
    Tag {
        tag_name: String,
    },
}

impl StoreRef {
    pub fn as_bookmark(&self) -> Result<&str> {
        match self {
            StoreRef::LocalBookmark { bookmark_name, .. } => Ok(bookmark_name),
            StoreRef::RemoteBookmark { bookmark_name, .. } => Ok(bookmark_name),
            _ => Err(anyhow!("not a local bookmark")),
        }
    }
}

/// Specifies which bookmarks/remotes to push or fetch
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum GitRefspec {
    AllBookmarks {
        remote_name: String,
    },
    AllRemotes {
        bookmark_ref: StoreRef,
    },
    RemoteBookmark {
        remote_name: String,
        bookmark_ref: StoreRef,
    },
}

/// Refers to one of the repository's manipulatable objects
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
#[allow(clippy::large_enum_variant)]
pub enum Operand {
    Repository,
    Revision {
        header: RevHeader,
    },
    Revisions {
        headers: Vec<RevHeader>,
    },
    Merge {
        header: RevHeader,
    },
    Parent {
        header: RevHeader,
        child: RevHeader,
    },
    Change {
        headers: Vec<RevHeader>,
        path: TreePath,
        hunk: Option<ChangeHunk>,
    },
    Ref {
        header: RevHeader,
        r#ref: StoreRef,
    },
}

/// A prompt for user input (e.g. credentials for git operations).
#[derive(Serialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct InputRequest {
    pub title: String,
    pub detail: String,
    pub fields: Vec<InputField>,
}

/// User-provided values for an [`InputRequest`].
#[derive(Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct InputResponse {
    pub fields: HashMap<String, String>,
}

/// A single field in an [`InputRequest`] form.
#[derive(Serialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct InputField {
    pub label: String,
    pub choices: Vec<String>,
}

impl From<&str> for InputField {
    fn from(label: &str) -> Self {
        InputField {
            label: label.to_owned(),
            choices: vec![],
        }
    }
}

/// Progress updates for long-running operations like git fetch/push.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum ProgressEvent {
    Progress { overall_percent: u32 },
    Message { text: String },
}
