use super::*;

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
    Updated {
        new_status: RepoStatus,
    },
    UpdatedSelection {
        new_status: RepoStatus,
        new_selection: RevHeader,
    },
    PreconditionError {
        message: String,
    },
    InternalError {
        message: MultilineString,
    },
}

/// Makes a revision the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CheckoutRevision {
    pub change_id: RevId,
}

/// Creates a new revision and makes it the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CreateRevision {
    pub parent_change_ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct InsertRevision {
    pub change_id: RevId,
    pub after_id: RevId,
    pub before_id: RevId,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveRevision {
    pub change_id: RevId,
    pub parent_ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveSource {
    pub change_id: RevId,
    pub parent_ids: Vec<RevId>,
}

/// Updates a revision's description
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DescribeRevision {
    pub change_id: RevId,
    pub new_description: String,
    pub reset_author: bool,
}

/// Creates a copy of the revision with the same parents and content
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DuplicateRevisions {
    pub change_ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct AbandonRevisions {
    pub commit_ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveChanges {
    pub from_id: RevId,
    pub to_id: RevId,
    pub paths: Vec<TreePath>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CopyChanges {
    pub from_id: RevId,
    pub to_id: RevId,
    pub paths: Vec<TreePath>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct TrackBranch {
    pub name: RefName,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct UntrackBranch {
    pub name: RefName,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveBranch {
    pub name: RefName,
    pub to_id: RevId,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct PushRemote {
    pub remote_name: String,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct FetchRemote {
    pub remote_name: String,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct UndoOperation;
