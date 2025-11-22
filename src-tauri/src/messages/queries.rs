use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc, offset::LocalResult};
use jj_lib::backend::{Signature, Timestamp};

use super::*;

/// A change or commit id with a disambiguated prefix
#[allow(dead_code)] // the frontend needs these structs kept in sync
pub trait Id {
    fn hex(&self) -> &String;
    fn prefix(&self) -> &String;
    fn rest(&self) -> &String;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct ChangeId {
    pub hex: String,
    pub prefix: String,
    pub rest: String,
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
/// The worker may use one or both depending on policy.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevId {
    pub change: ChangeId,
    pub commit: CommitId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevHeader {
    pub id: RevId,
    pub description: MultilineString,
    pub author: RevAuthor,
    pub has_conflict: bool,
    pub is_working_copy: bool,
    pub is_immutable: bool,
    pub refs: Vec<StoreRef>,
    pub parent_ids: Vec<CommitId>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevChange {
    pub kind: ChangeKind,
    pub path: TreePath,
    pub has_conflict: bool,
    pub hunks: Vec<ChangeHunk>,
}

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevConflict {
    pub path: TreePath,
    pub hunk: ChangeHunk,
}

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum ChangeKind {
    None,
    Added,
    Deleted,
    Modified,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct ChangeHunk {
    pub location: HunkLocation,
    pub lines: MultilineString,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct HunkLocation {
    pub from_file: FileRange,
    pub to_file: FileRange,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct FileRange {
    pub start: usize,
    pub len: usize,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
#[allow(clippy::large_enum_variant)]
pub enum RevResult {
    NotFound {
        id: RevId,
    },
    Detail {
        header: RevHeader,
        parents: Vec<RevHeader>,
        changes: Vec<RevChange>,
        conflicts: Vec<RevConflict>,
    },
}

#[derive(Serialize, Clone, Copy, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct LogCoordinates(pub usize, pub usize);

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
    ToMissing {
        source: LogCoordinates,
        target: LogCoordinates,
        indirect: bool,
    },
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
