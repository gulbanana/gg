use super::*;

/// A change or commit id with a disambiguated prefix
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
    pub branches: Vec<RefName>,
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

impl From<&Signature> for RevAuthor {
    fn from(value: &Signature) -> Self {
        RevAuthor {
            name: value.name.clone(),
            email: value.email.clone(),
            timestamp: datetime_from_timestamp(&value.timestamp)
                .expect("convert timestamp to datetime")
                .with_timezone(&Local),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RevChange {
    pub kind: ChangeKind,
    pub path: TreePath,
    pub has_conflict: bool,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum ChangeKind {
    Added,
    Deleted,
    Modified,
}

#[derive(Serialize)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum RevResult {
    NotFound {
        id: RevId,
    },
    Detail {
        header: RevHeader,
        parents: Vec<RevHeader>,
        changes: Vec<RevChange>,
        conflicts: Vec<TreePath>,
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
            &FixedOffset::east_opt(context.tz_offset * 60).unwrap_or_else(|| {
                FixedOffset::east_opt(0).expect("timezone offset out of bounds")
            }),
        ),
    )
}
