mod mutations;
mod queries;
mod session;

use anyhow::Result;
use jj_lib::{
    backend::TreeValue, commit::Commit, repo::Repo as _, repo_path::RepoPath,
    revset::RevsetIteratorExt,
};
use std::{
    fs::{self, File},
    path::PathBuf,
    sync::Arc,
};
use tempfile::{TempDir, tempdir};
use zip::ZipArchive;

use crate::{
    messages::{ChangeId, CommitId, RevId, RevSet, RevsResult},
    worker::{EventSink, WorkerSession, WorkspaceSession, queries::query_revisions},
};

pub struct NoProgress;

impl EventSink for NoProgress {
    fn send(&self, _event_name: &str, _payload: serde_json::Value) {}
}

impl Default for WorkerSession {
    fn default() -> Self {
        WorkerSession {
            force_log_page_size: None,
            latest_query: None,
            working_directory: None,
            user_settings: crate::config::tests::settings_with_gg_defaults(),
            sink: Arc::new(NoProgress),
        }
    }
}

// Test Repository Maintenance
// ==========================
// The test repository is stored as `res/test-repo.zip` and extracted by `mkrepo()`.
//
// To modify the test repository:
// 1. Extract test-repo.zip to a temporary directory
// 2. Use `jj` CLI commands to create/modify commits
// 3. Verify new commits are mutable: `jj log -r 'mutable()'`
// 4. Re-zip the directory (excluding any OS-specific files)
// 5. Update the `revs` module with new commit IDs
//
// The `revs` module provides helpers for known commits. Use `jj log` to find change/commit IDs.

fn mkrepo() -> TempDir {
    let repo_dir = tempdir().unwrap();
    let mut archive_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    archive_path.push("res/test-repo.zip");
    let archive_file = File::open(&archive_path).unwrap();
    let mut archive = ZipArchive::new(archive_file).unwrap();

    archive.extract(repo_dir.path()).unwrap();

    repo_dir
}

fn mkid(xid: &str, cid: &str) -> RevId {
    RevId {
        change: ChangeId {
            hex: xid.to_owned(),
            prefix: xid.to_owned(),
            rest: "".to_owned(),
        },
        commit: CommitId {
            hex: cid.to_owned(),
            prefix: cid.to_owned(),
            rest: "".to_owned(),
        },
    }
}

/// Resolve a commit by change ID, even if it was rewritten and has a new commit ID.
/// Use this to verify commit state after mutations that rewrite commits.
fn get_rev(ws: &WorkspaceSession, rev_id: &RevId) -> Result<Commit> {
    use jj_lib::repo::Repo;
    use jj_lib::revset::RevsetIteratorExt;

    let revset = ws.evaluate_revset_str(&rev_id.change.hex)?;
    let mut iter = revset.as_ref().iter().commits(ws.repo().store()).fuse();
    match iter.next() {
        Some(commit) => Ok(commit?),
        None => anyhow::bail!("Change {} not found", rev_id.change.hex),
    }
}

async fn query_by_chid(ws: &WorkspaceSession<'_>, change_hex: &str) -> Result<RevsResult> {
    let revset = ws.evaluate_revset_str(change_hex)?;
    let commits: Vec<_> = revset
        .iter()
        .commits(ws.repo().store())
        .collect::<Result<Vec<_>, _>>()?;
    let commit = commits
        .first()
        .ok_or_else(|| anyhow::anyhow!("not found"))?;
    let id = ws.format_id(commit);
    query_by_id(ws, id).await
}

/// Helper to get a single revision's display details (changes, conflicts, etc.)
async fn query_by_id(
    ws: &crate::worker::gui_util::WorkspaceSession<'_>,
    id: RevId,
) -> Result<RevsResult> {
    query_revisions(ws, RevSet::singleton(id)).await
}

mod revs {
    use crate::messages::RevId;

    use super::mkid;

    /// The working copy commit (empty, child of main)
    pub fn working_copy() -> RevId {
        mkid("kvptxrkr", "e7080cd830960125c13e276aa056c811e7ce600a")
    }

    /// The main bookmark commit (renamed c.txt)
    pub fn main_bookmark() -> RevId {
        mkid("wnpusytq", "025843422c8f5374a4160fe79195b92d6ec3c6ee")
    }

    /// Bookmark added to immutable_heads()
    pub fn immutable_bookmark() -> RevId {
        mkid("ywknyuol", "f86298e8166104062708cde7c1cf697022b4cf8b")
    }

    /// An immutable commit (parent of immutable_bookmark)
    pub fn immutable_parent() -> RevId {
        mkid("nxxylmpu", "fa32b17fcc7f44f176539feec6c13af413924329")
    }

    /// An immutable commit (grandparent of immutable_bookmark)
    pub fn immutable_grandparent() -> RevId {
        mkid("tqnnuvwv", "983d594962e861aa155c8cee9e49122978cec40f")
    }

    /// A commit with a conflict in b.txt
    pub fn conflict_bookmark() -> RevId {
        mkid("pkullrwy", "18edcaea9423cd9975c3f1ffbf07e00fe3ecc47a")
    }

    /// Child of conflict_bookmark that resolves the conflict
    pub fn resolve_conflict() -> RevId {
        mkid("yvtwywll", "461b914dbab3347a7c789bac200f0e135d03807e")
    }

    /// Child of conflict_bookmark that does NOT resolve the conflict
    /// Adds unrelated.txt but keeps b.txt in conflict state
    pub fn inherited_conflict() -> RevId {
        mkid("tlxnptkw", "7241ca5bfef9f77eccb9544f8a69c61025d766c1")
    }

    /// Merge commit that introduces conflict in conflict_chain.txt
    /// Child of resolve_conflict via two branches (chain branch A and B)
    pub fn chain_conflict() -> RevId {
        mkid("vwxxopnk", "f80d4defdcf8627e7e8dca52fefb250e2e05d133")
    }

    /// Child of chain_conflict that resolves the conflict in conflict_chain.txt
    pub fn chain_resolved() -> RevId {
        mkid("lwzoqltx", "8c812d8bacb3ccb4ce4a3eff30e1221eef3373ca")
    }

    /// Mutable commit that changed b.txt from "1" to "11"
    pub fn hunk_source() -> RevId {
        mkid("xoooutru", "1b3949ce69432a74966165308ac30f5501fd9a83")
    }

    /// Contains hunk_test.txt with 5 lines: line1-line5
    pub fn hunk_base() -> RevId {
        mkid("xrqnzmzy", "71627400c7459f17fa45ea5dfd2572830f5c26ab")
    }

    /// Child of hunk_base, modifies line 2: line2 -> modified2
    pub fn hunk_child_single() -> RevId {
        mkid("rwpmyumq", "cb56950fd81e14bcf30ea657f3c69a99ca743229")
    }

    /// Child of hunk_base, modifies lines 2 and 4: line2 -> changed2, line4 -> changed4
    pub fn hunk_child_multi() -> RevId {
        mkid("nwywsplo", "b234894cba9641611cbd3e0648dd2ac3c634c272")
    }

    /// Child of hunk_base, adds lines 6-8: new6, new7, new8
    pub fn hunk_sibling() -> RevId {
        mkid("lpvoqxrx", "489cf8d28d84c3477c65f89a856ba70ac91081bb")
    }

    /// Child of hunk_child_single, modifies line 3: line3 -> grandchild3
    /// This creates a 3-level hierarchy: hunk_base -> hunk_child_single -> hunk_grandchild
    pub fn hunk_grandchild() -> RevId {
        mkid("onsonsrz", "1c073dfca738cdca246a1f8818f8f67bb3b4c8e6")
    }

    /// Contains small.txt with 2 lines: line1, line2
    pub fn small_parent() -> RevId {
        mkid("uqpmkpqu", "cd1a7fc72d71051f3a336a40da45d01d1d1a624c")
    }

    /// Child of small_parent, modifies line 2: line2 -> changed
    pub fn small_child() -> RevId {
        mkid("vnstymnv", "f08d8a81983eb0c7849359b1555dca2d93016b54")
    }
}

#[test]
fn wc_path_is_visible() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let commit = ws.get_commit(ws.wc_id())?;
    let value = commit
        .tree()
        .path_value(RepoPath::from_internal_string("a.txt")?)?;

    assert!(value.is_resolved());
    assert!(
        value
            .first()
            .as_ref()
            .is_some_and(|x| matches!(x, TreeValue::File { .. }))
    );

    Ok(())
}

#[tokio::test]
async fn snapshot_updates_wc_if_changed() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;
    let old_wc = ws.wc_id().clone();

    assert!(!ws.import_and_snapshot(true, false).await?);
    assert_eq!(&old_wc, ws.wc_id());

    fs::write(repo.path().join("new.txt"), []).unwrap();

    assert!(ws.import_and_snapshot(true, false).await?);
    assert_ne!(&old_wc, ws.wc_id());

    Ok(())
}

#[tokio::test]
async fn transaction_updates_wc_if_snapshot() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;
    let old_wc = ws.wc_id().clone();

    fs::write(repo.path().join("new.txt"), []).unwrap();

    let tx = ws.start_transaction().await?;
    ws.finish_transaction(tx, "do nothing")?;

    assert_ne!(&old_wc, ws.wc_id());

    Ok(())
}

#[tokio::test]
async fn transaction_snapshot_path_is_visible() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    fs::write(repo.path().join("new.txt"), []).unwrap();

    let tx = ws.start_transaction().await?;
    ws.finish_transaction(tx, "do nothing")?;

    let commit = ws.get_commit(ws.wc_id())?;
    let value = commit
        .tree()
        .path_value(RepoPath::from_internal_string("new.txt")?)?;

    assert!(value.is_resolved());
    assert!(
        value
            .first()
            .as_ref()
            .is_some_and(|x| matches!(x, TreeValue::File { .. }))
    );

    Ok(())
}
