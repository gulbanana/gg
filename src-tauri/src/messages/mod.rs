//! Message types used to communicate between backend and frontend

mod mutations;
mod queries;

pub use mutations::*;
pub use queries::*;

use std::{collections::HashMap, path::Path};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-rs")]
use ts_rs::TS;

/// Utility type used to abstract crlf/<br>/etc
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
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
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
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
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct TreePath {
    pub repo_path: String,
    pub relative_path: DisplayPath,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]

pub enum RepoConfig {
    #[allow(dead_code)] // used by frontend
    Initial,
    Workspace {
        absolute_path: DisplayPath,
        git_remotes: Vec<String>,
        default_query: String,
        latest_query: String,
        status: RepoStatus,
        theme_override: Option<String>,
        mark_unpushed_branches: bool,
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

#[derive(Serialize, Clone, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RepoStatus {
    pub operation_description: String,
    pub working_copy: CommitId,
}

/// Bookmark or tag name with metadata.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum StoreRef {
    LocalBookmark {
        branch_name: String,
        has_conflict: bool,
        /// Synchronized with all tracking remotes
        is_synced: bool,
        /// Actual and potential remotes
        tracking_remotes: Vec<String>,
        available_remotes: usize,
        potential_remotes: usize,
    },
    RemoteBookmark {
        branch_name: String,
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
    pub fn as_branch(&self) -> Result<&str> {
        match self {
            StoreRef::LocalBookmark { branch_name, .. } => Ok(&branch_name),
            StoreRef::RemoteBookmark { branch_name, .. } => Ok(&branch_name),
            _ => Err(anyhow!("not a local bookmark")),
        }
    }
}

/// Refers to one of the repository's manipulatable objects
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum Operand {
    Repository,
    Revision {
        header: RevHeader,
    },
    Merge {
        header: RevHeader,
    },
    Parent {
        header: RevHeader,
        child: RevHeader,
    },
    Change {
        header: RevHeader,
        path: TreePath, // someday: hunks
    },
    Ref {
        header: RevHeader,
        r#ref: StoreRef,
    },
    Hunk {
        header: RevHeader,
        path: String,
        hunk: ChangeHunk,
        conflicted: bool,
    },
}

#[derive(Serialize, Debug, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct InputRequest {
    pub title: String,
    pub detail: String,
    pub fields: Vec<InputField>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct InputResponse {
    pub cancel: bool,
    pub fields: HashMap<String, String>,
}

#[derive(Serialize, Debug, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
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
