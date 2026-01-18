use super::*;

/// Common result type for mutating commands
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
#[allow(clippy::large_enum_variant)]
pub enum MutationResult {
    Unchanged,
    Updated {
        new_status: RepoStatus,
        new_selection: Option<RevHeader>,
    },
    Reconfigured {
        new_config: RepoConfig,
    },
    InputRequired {
        request: InputRequest,
    },
    PreconditionError {
        message: String,
    },
    InternalError {
        message: MultilineString,
    },
}

/// Global pseudomutation: creates a repository from a URL
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CloneRepository {
    pub url: String,
    pub path: String,
    pub colocated: bool,
}

/// Global pseudomutation: creates a repository
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct InitRepository {
    pub path: String,
    pub colocated: bool,
}

/// Makes a revision the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CheckoutRevision {
    pub id: RevId,
}

/// Creates a new revision and makes it the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CreateRevision {
    pub set: RevSet,
}

/// Creates a new revision between two changes and makes it the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CreateRevisionBetween {
    pub after_id: CommitId,
    pub before_id: RevId,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct InsertRevisions {
    pub set: RevSet,
    pub after_id: RevId,
    pub before_id: RevId,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveRevisions {
    pub set: RevSet,
    pub parent_ids: Vec<RevId>,
}

/// Sets a revision's parents (used to add/remove parents from merge commits)
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct AdoptRevision {
    pub id: RevId,
    pub parent_ids: Vec<CommitId>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveHunk {
    pub from_id: RevId,
    pub to_id: CommitId,
    pub path: TreePath,
    pub hunk: ChangeHunk,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CopyHunk {
    pub from_id: CommitId, // limitation: we don't know parent chids because they are more expensive to look up
    pub to_id: RevId,
    pub path: TreePath,
    pub hunk: ChangeHunk,
}

/// Updates a revision's description
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct DescribeRevision {
    pub id: RevId,
    pub new_description: String,
    pub reset_author: bool,
}

/// Creates a copy of the selected revisions with the same parents and content
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct DuplicateRevisions {
    pub set: RevSet,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct AbandonRevisions {
    pub set: RevSet,
}

/// Adds changes to the working copy which reverse the effect of the selected revisions
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct BackoutRevisions {
    pub set: RevSet,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveChanges {
    pub from: RevSet,
    pub to_id: CommitId, // limitation: we don't know parent change ids because they are more expensive to look up
    pub paths: Vec<TreePath>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CopyChanges {
    pub from_id: CommitId, // limitation: we don't know parent change ids because they are more expensive to look up
    pub to_set: RevSet,
    pub paths: Vec<TreePath>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct TrackBookmark {
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct UntrackBookmark {
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RenameBookmark {
    pub r#ref: StoreRef,
    pub new_name: String,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CreateRef {
    pub id: RevId,
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct DeleteRef {
    pub r#ref: StoreRef,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveRef {
    pub r#ref: StoreRef,
    pub to_id: RevId,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct GitPush {
    pub refspec: GitRefspec,
    #[serde(default)]
    pub input: Option<InputResponse>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct GitFetch {
    pub refspec: GitRefspec,
    /// Response from a previous InputRequired.
    /// If None and credentials are needed, the mutation returns InputRequired.
    #[serde(default)]
    pub input: Option<InputResponse>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct UndoOperation;
