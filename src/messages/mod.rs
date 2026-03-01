//! Serializable message types shared between the Rust backend and the
//! TypeScript frontend.
//!
//! Every type here derives [`Serialize`] and/or
//! [`Deserialize`]. When the `ts-rs` feature is enabled,
//! `cargo gen` (alias for `cargo test -F ts-rs`) exports matching TypeScript
//! definitions into `app/messages/`.

pub mod mutations;
pub mod queries;

use std::{collections::HashMap, path::Path};

use anyhow::{Result, anyhow};
use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc, offset::LocalResult};
use jj_lib::backend::{Signature, Timestamp};
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

/// A contiguous range of lines in a file.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ChangeRange {
    pub start: usize,
    pub len: usize,
}

/// A single contiguous diff hunk with its location and content.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ChangeHunk {
    pub location: ChangeLocation,
    pub lines: MultilineString,
}

/// Line ranges in the source and target files for a diff hunk.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ChangeLocation {
    pub from_file: ChangeRange,
    pub to_file: ChangeRange,
}

/// A change or commit id with a disambiguated prefix
#[allow(dead_code)] // the frontend needs these structs kept in sync
pub trait Id {
    fn hex(&self) -> &String;
    fn prefix(&self) -> &String;
    fn rest(&self) -> &String;
}

/// A commit's unique hash identifier with disambiguated prefix.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CommitId {
    pub hex: String,
    pub prefix: String,
    pub rest: String,
}

impl Id for CommitId {
    fn hex(&self) -> &String {
        &self.hex
    }
    fn prefix(&self) -> &String {
        &self.prefix
    }
    fn rest(&self) -> &String {
        &self.rest
    }
}

/// A change's unique identifier with disambiguated prefix.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ChangeId {
    pub hex: String,
    pub prefix: String,
    pub rest: String,
    pub offset: Option<usize>,
    pub is_divergent: bool,
}

impl Id for ChangeId {
    fn hex(&self) -> &String {
        &self.hex
    }
    fn prefix(&self) -> &String {
        &self.prefix
    }
    fn rest(&self) -> &String {
        &self.rest
    }
}

/// A pair of ids representing the ui's view of a revision.
///
/// The worker may use one or both depending on policy.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevId {
    pub change: ChangeId,
    pub commit: CommitId,
}

/// A sequence (specifically) of revision ids.
///
/// Equivalent to either `from::to` or `to::from` - whichever one is nonempty.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevSet {
    pub from: RevId,
    pub to: RevId,
}

#[cfg(test)]
impl RevSet {
    pub fn singleton(id: RevId) -> Self {
        Self {
            from: id.clone(),
            to: id,
        }
    }

    pub fn sequence(from: RevId, to: RevId) -> Self {
        Self { from, to }
    }
}

/// A revision's author name, email, and timestamp.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevAuthor {
    pub email: String,
    pub name: String,
    pub timestamp: chrono::DateTime<Local>,
}

impl TryFrom<&Signature> for RevAuthor {
    type Error = anyhow::Error;

    fn try_from(value: &Signature) -> Result<RevAuthor> {
        Ok(RevAuthor {
            name: value.name.clone(),
            email: value.email.clone(),
            timestamp: format_timestamp(&value.timestamp)?.with_timezone(&Local),
        })
    }
}

/// Summary metadata for a revision displayed in the log.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevHeader {
    pub id: RevId,
    pub description: MultilineString,
    pub author: RevAuthor,
    pub has_conflict: bool,
    pub is_working_copy: bool,
    pub working_copy_of: Option<String>,
    pub is_immutable: bool,
    pub refs: Vec<StoreRef>,
    pub parent_ids: Vec<CommitId>,
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

// similar to time_util::datetime_from_timestamp, which is not pub
fn format_timestamp(context: &Timestamp) -> Result<DateTime<FixedOffset>> {
    let utc = match Utc.timestamp_opt(
        context.timestamp.0.div_euclid(1000),
        (context.timestamp.0.rem_euclid(1000)) as u32 * 1000000,
    ) {
        LocalResult::None => {
            return Err(anyhow!("no UTC instant exists for timestamp"));
        }
        LocalResult::Single(x) => x,
        LocalResult::Ambiguous(y, _z) => y,
    };

    let tz = FixedOffset::east_opt(context.tz_offset * 60)
        .or_else(|| FixedOffset::east_opt(0))
        .ok_or(anyhow!("timezone offset out of bounds"))?;

    Ok(utc.with_timezone(&tz))
}
