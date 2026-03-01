//! Request types that modify the repository. Each mutation struct implements
//! the [`Mutation`](crate::worker::Mutation) trait, which defines how the
//! change is executed on the worker thread.

use super::*;

/// Common options for all mutations
#[derive(Deserialize, Debug, Default)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MutationOptions {
    pub ignore_immutable: bool,
}

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

/// Specifies which bookmarks/remotes to push or fetch
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub enum GitRefspec {
    AllBookmarks {
        remote_name: String,
    },
    AllRemotes {
        bookmark_ref: StoreRef,
    },
    RemoteBookmark {
        remote_name: String,
        bookmark_ref: StoreRef,
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

/// Inserts revisions between two points in the graph.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct InsertRevisions {
    pub set: RevSet,
    pub after_id: RevId,
    pub before_id: RevId,
}

/// Rebases revisions onto new parents.
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

/// Moves a single diff hunk from one revision to another.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveHunk {
    pub from_id: RevId,
    pub to_id: CommitId,
    pub path: TreePath,
    pub hunk: ChangeHunk,
}

/// Copies a single diff hunk from one revision into another.
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

/// Abandons revisions, rebasing their children onto their parents.
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

/// Moves file-level changes from one revision to another.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveChanges {
    pub from: RevSet,
    pub to_id: CommitId, // limitation: we don't know parent change ids because they are more expensive to look up
    pub paths: Vec<TreePath>,
}

/// Copies file-level changes from one revision into another.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CopyChanges {
    pub from_id: CommitId, // limitation: we don't know parent change ids because they are more expensive to look up
    pub to_set: RevSet,
    pub paths: Vec<TreePath>,
}

/// Starts tracking a remote bookmark locally.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct TrackBookmark {
    pub r#ref: StoreRef,
}

/// Stops tracking a remote bookmark locally.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct UntrackBookmark {
    pub r#ref: StoreRef,
}

/// Renames a local bookmark.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct RenameBookmark {
    pub r#ref: StoreRef,
    pub new_name: String,
}

/// Creates a new bookmark or tag on a revision.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct CreateRef {
    pub id: RevId,
    pub r#ref: StoreRef,
}

/// Deletes a bookmark or tag.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct DeleteRef {
    pub r#ref: StoreRef,
}

/// Moves a bookmark or tag to a different revision.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct MoveRef {
    pub r#ref: StoreRef,
    pub to_id: RevId,
}

/// Pushes bookmarks to a git remote.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct GitPush {
    pub refspec: GitRefspec,
    #[serde(default)]
    pub input: Option<InputResponse>,
}

/// Fetches bookmarks from a git remote.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct GitFetch {
    pub refspec: GitRefspec,
    /// Response from a previous InputRequired.
    /// If None and credentials are needed, the mutation returns InputRequired.
    #[serde(default)]
    pub input: Option<InputResponse>,
}

/// Reverts the most recent jj operation.
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct UndoOperation;

/// Opens a file's diff in the user's configured external diff tool
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ExternalDiff {
    pub id: RevId,
    pub path: TreePath,
}

/// Resolves a file's conflict in the user's configured external merge tool
#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "ts-rs", derive(TS), ts(export, export_to = "app/messages/"))]
pub struct ExternalResolve {
    pub id: RevId,
    pub path: TreePath,
}
