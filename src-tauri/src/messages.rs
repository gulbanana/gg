//! Message types used to communicate between backend and frontend

use std::path::PathBuf;

use chrono::Local;
use serde::Serialize;
use ts_rs::TS;

/// Utility type used to abstract crlf/<br>/etc
#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
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
#[derive(TS, Serialize, Clone)]
#[ts(export, export_to = "../src/messages/")]
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

#[derive(TS, Serialize, Clone)]
#[ts(export, export_to = "../src/messages/")]
pub struct RepoConfig {
    pub absolute_path: DisplayPath,
    pub default_revset: String,
    pub status: RepoStatus,
}

#[derive(TS, Serialize, Clone)]
#[ts(export, export_to = "../src/messages/")]
pub struct RepoStatus {
    pub operation_description: String,
    pub working_copy: RevId,
}

#[derive(TS, Serialize, Clone, Copy)]
#[ts(export, export_to = "../src/messages/")]
pub struct LogCoordinates(pub usize, pub usize);

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
#[serde(tag = "type")]
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

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct LogRow {
    pub location: LogCoordinates,
    pub padding: usize,
    pub revision: RevHeader,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct LogPage {
    pub rows: Vec<LogRow>,
    pub lines: Vec<LogLine>,
    pub has_more: bool,
}

/// A change or commit id with a disambiguated prefix
#[derive(TS, Serialize, Clone)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevId {
    pub prefix: String,
    pub rest: String,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevHeader {
    pub change_id: RevId,
    pub commit_id: RevId,
    pub description: MultilineString,
    pub author: String,
    pub email: String,
    pub timestamp: chrono::DateTime<Local>,
    pub has_conflict: bool,
    pub branches: Vec<RefName>,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevDetail {
    pub header: RevHeader,
    pub parents: Vec<RevHeader>,
    pub diff: Vec<DiffPath>,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
#[serde(tag = "type")]
pub enum DiffPath {
    Added { relative_path: DisplayPath },
    Deleted { relative_path: DisplayPath },
    Modified { relative_path: DisplayPath },
}

/// Branch or tag name with metadata.
#[derive(TS, Serialize, Clone)]
#[ts(export, export_to = "../src/messages/")]
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
