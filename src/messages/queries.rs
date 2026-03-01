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
/// The worker may use one or both depending on policy.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevId {
    pub change: ChangeId,
    pub commit: CommitId,
}

/// A sequence (specifically) of revision ids. Equivalent to either `from::to` or `to::from` - whichever one is nonempty.
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

/// A file-level change within a revision's diff.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevChange {
    pub kind: ChangeKind,
    pub path: TreePath,
    pub has_conflict: bool,
    pub hunks: Vec<ChangeHunk>,
}

/// A file-level conflict in a revision's parent tree.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RevConflict {
    pub path: TreePath,
    pub hunk: ChangeHunk,
}

/// The type of modification made to a file in a diff.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum ChangeKind {
    None,
    Added,
    Deleted,
    Modified,
}

/// A single contiguous diff hunk with its location and content.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ChangeHunk {
    pub location: HunkLocation,
    pub lines: MultilineString,
}

/// Line ranges in the source and target files for a diff hunk.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct HunkLocation {
    pub from_file: FileRange,
    pub to_file: FileRange,
}

/// A contiguous range of lines in a file.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct FileRange {
    pub start: usize,
    pub len: usize,
}

/// Response to a revision detail query: either not found or full details.
#[derive(Serialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
#[allow(clippy::large_enum_variant)]
pub enum RevsResult {
    NotFound {
        set: RevSet,
    },
    Detail {
        /// The revision set that was queried.
        set: RevSet,
        /// All revisions in the set, ordered ancestors-first.
        headers: Vec<RevHeader>,
        /// Parents of the oldest revision in the set.
        parents: Vec<RevHeader>,
        /// Combined changes: diff from oldest parent to newest revision.
        changes: Vec<RevChange>,
        /// Conflicts present in the parent tree of the oldest revision.
        conflicts: Vec<RevConflict>,
    },
}

/// (column, row) position of a node in the log graph.
#[derive(Serialize, Clone, Copy, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct LogCoordinates(pub usize, pub usize);

/// An edge segment in the log graph connecting two nodes.
#[derive(Serialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
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

/// A single row in the log graph with its revision and edges.
#[derive(Serialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct LogRow {
    pub revision: RevHeader,
    pub location: LogCoordinates,
    pub padding: usize,
    pub lines: Vec<LogLine>,
}

/// A paginated slice of the log graph.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
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
