//! Message types used to communicate between backend and frontend

use std::path::PathBuf;

use chrono::Local;
use serde::Serialize;
#[cfg(feature = "ts-rs")]
use ts_rs::TS;

/// Utility type used to abstract crlf/<br>/etc
#[derive(Serialize)]
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
        default_revset: String,
        status: RepoStatus,
    },
    NoWorkspace {
        absolute_path: DisplayPath,
        error: String,
    },
    NoOperation {
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

#[derive(Serialize)]
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

#[derive(Serialize)]
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

#[derive(Serialize, Clone, Copy)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct LogCoordinates(pub usize, pub usize);

/// A change or commit id with a disambiguated prefix
#[derive(Serialize, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevId {
    pub prefix: String,
    pub rest: String,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevHeader {
    pub change_id: RevId,
    pub commit_id: RevId,
    pub description: MultilineString,
    pub has_conflict: bool,
    pub is_working_copy: bool,
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
    pub author: String,
    pub email: String,
    pub timestamp: chrono::DateTime<Local>,
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
#[derive(Serialize, Clone)]
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
