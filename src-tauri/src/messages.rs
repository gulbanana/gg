//! Message types used to communicate between backend and frontend

use std::path::PathBuf;

use chrono::{DateTime, FixedOffset, Local, LocalResult, TimeZone, Utc};
use jj_lib::backend::{Signature, Timestamp};
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-rs")]
use ts_rs::TS;

/// Utility type used to abstract crlf/<br>/etc
#[derive(Serialize, Debug)]
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
#[derive(Serialize, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DisplayPath(String);

impl From<&PathBuf> for DisplayPath {
    fn from(value: &PathBuf) -> Self {
        DisplayPath(
            value
                .to_string_lossy()
                .trim_start_matches("\\\\?\\")
                .to_owned(),
        )
    }
}

/// Common result type for mutating commands
#[derive(Serialize, Clone)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum MutationResult {
    Unchanged,
    Updated { new_status: RepoStatus },
    Failed { message: String },
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

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct LogPage {
    pub rows: Vec<LogRow>,
    pub has_more: bool,
}

#[derive(Serialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct LogRow {
    pub revision: RevHeader,
    pub location: LogCoordinates,
    pub padding: usize,
    pub lines: Vec<LogLine>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum LogLine {
    FromNode {
        source: LogCoordinates,
        target: LogCoordinates,
        indirect: bool,
    },
    ToNode {
        source: LogCoordinates,
        target: LogCoordinates,
        indirect: bool,
    },
    ToIntersection {
        source: LogCoordinates,
        target: LogCoordinates,
        indirect: bool,
    },
}

#[derive(Serialize, Clone, Copy, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct LogCoordinates(pub usize, pub usize);

/// A change or commit id with a disambiguated prefix
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevId {
    pub hex: String,
    pub prefix: String,
    pub rest: String,
}

#[derive(Serialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevAuthor {
    pub email: String,
    pub name: String,
    pub timestamp: chrono::DateTime<Local>,
}

impl From<&Signature> for RevAuthor {
    fn from(value: &Signature) -> Self {
        RevAuthor {
            name: value.name.clone(),
            email: value.email.clone(),
            timestamp: datetime_from_timestamp(&value.timestamp)
                .unwrap()
                .with_timezone(&Local),
        }
    }
}

#[derive(Serialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevHeader {
    pub change_id: RevId,
    pub commit_id: RevId,
    pub description: MultilineString,
    pub author: RevAuthor,
    pub has_conflict: bool,
    pub is_working_copy: bool,
    pub is_immutable: bool,
    pub branches: Vec<RefName>,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevDetail {
    pub header: RevHeader,
    pub parents: Vec<RevHeader>,
    pub diff: Vec<DiffPath>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum DiffPath {
    Added { relative_path: DisplayPath },
    Deleted { relative_path: DisplayPath },
    Modified { relative_path: DisplayPath },
}

/// Branch or tag name with metadata.
#[derive(Serialize, Clone, Debug)]
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

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DescribeRevision {
    pub change_id: RevId,
    pub new_description: String,
}

// from time_util, which is not pub
fn datetime_from_timestamp(context: &Timestamp) -> Option<DateTime<FixedOffset>> {
    let utc = match Utc.timestamp_opt(
        context.timestamp.0.div_euclid(1000),
        (context.timestamp.0.rem_euclid(1000)) as u32 * 1000000,
    ) {
        LocalResult::None => {
            return None;
        }
        LocalResult::Single(x) => x,
        LocalResult::Ambiguous(y, _z) => y,
    };

    Some(
        utc.with_timezone(
            &FixedOffset::east_opt(context.tz_offset * 60)
                .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap()),
        ),
    )
}
