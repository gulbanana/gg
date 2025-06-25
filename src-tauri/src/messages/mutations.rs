use super::*;

/// Common result type for mutating commands
#[derive(Serialize, Clone, Debug)]
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
    pub id: RevId,
}

/// Creates a new revision and makes it the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CreateRevision {
    pub parent_ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct InsertRevision {
    pub id: RevId,
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
    pub id: RevId,
    pub parent_ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveSource {
    pub id: RevId,
    pub parent_ids: Vec<CommitId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveHunk {
    pub from_id: RevId,
    pub to_id: CommitId,
    pub path: TreePath,
    pub hunk: ChangeHunk,
}

/// Updates a revision's description
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DescribeRevision {
    pub id: RevId,
    pub new_description: String,
    pub reset_author: bool,
}

/// Creates a copy of the selected revisions with the same parents and content
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DuplicateRevisions {
    pub ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct AbandonRevisions {
    pub ids: Vec<CommitId>,
}

/// Adds changes to the working copy which reverse the effect of the selected revisions
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct BackoutRevisions {
    pub ids: Vec<RevId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveChanges {
    pub from_id: RevId,
    pub to_id: CommitId, // limitation: we don't know parent chids because they are more expensive to look up
    pub paths: Vec<TreePath>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CopyChanges {
    pub from_id: CommitId, // limitation: we don't know parent chids because they are more expensive to look up
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
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct UntrackBranch {
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct RenameBranch {
    pub r#ref: StoreRef,
    pub new_name: String,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CreateRef {
    pub id: RevId,
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DeleteRef {
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct MoveRef {
    pub r#ref: StoreRef,
    pub to_id: RevId,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum GitPush {
    AllBookmarks {
        remote_name: String,
    },
    AllRemotes {
        branch_ref: StoreRef,
    },
    RemoteBookmark {
        remote_name: String,
        branch_ref: StoreRef,
    },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum GitFetch {
    AllBookmarks {
        remote_name: String,
    },
    AllRemotes {
        branch_ref: StoreRef,
    },
    RemoteBookmark {
        remote_name: String,
        branch_ref: StoreRef,
    },
}

#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct UndoOperation;
