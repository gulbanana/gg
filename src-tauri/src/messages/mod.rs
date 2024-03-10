//! Message types used to communicate between backend and frontend

mod mutations;
mod queries;

pub use mutations::*;
pub use queries::*;

use std::path::Path;

use chrono::{DateTime, FixedOffset, Local, LocalResult, TimeZone, Utc};
use jj_lib::backend::{Signature, Timestamp};
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
pub struct DisplayPath(String);

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
    Workspace {
        absolute_path: DisplayPath,
        default_query: String,
        latest_query: String,
        status: RepoStatus,
        theme: Option<String>,
    },
    NoWorkspace {
        absolute_path: DisplayPath,
        error: String,
    },
    DeadWorker {
        error: String,
    },
}

#[derive(Serialize, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RepoStatus {
    pub operation_description: String,
    pub working_copy: RevId,
}

/// Branch or tag name with metadata.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RefName {
    /// Local name.
    pub name: String,
    /// Remote name if this is a remote or Git-tracking ref.
    pub remote: Option<String>,
    /// Ref target has conflicts.
    pub has_conflict: bool,
    /// Local ref is synchronized with all tracking remotes, or tracking remote
    /// ref is synchronized with the local.
    pub is_synced: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum MenuContext {
    Revision {
        rev: RevHeader,
    },
    Tree {
        rev: RevHeader,
        path: TreePath, // someday: paths
    },
    Branch {
        rev: RevHeader,
        name: RefName,
    },
}
