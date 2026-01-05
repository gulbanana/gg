mod mutations;
mod queries;
mod session;

use anyhow::Result;
use jj_lib::{backend::TreeValue, commit::Commit, repo_path::RepoPath};
use std::{
    fs::{self, File},
    path::PathBuf,
    sync::Arc,
};
use tempfile::{TempDir, tempdir};
use zip::ZipArchive;

use crate::{
    messages::{ChangeId, CommitId, RevId},
    worker::{EventSink, WorkerSession, WorkspaceSession},
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

mod revs {
    use crate::messages::RevId;

    use super::mkid;

    /// The working copy commit (empty, child of main)
    pub fn working_copy() -> RevId {
        mkid("qtvuvvns", "79943c33f366b4c5a47ee6aac93a4074ebe155a4")
    }

    /// The main bookmark commit (renamed c.txt)
    pub fn main_bookmark() -> RevId {
        mkid("mnkoropy", "5f4dc18dd03158f9ec72e528ffaca2f4c73d3b4b")
    }

    /// Bookmark added to immutable_heads()
    pub fn immutable_bookmark() -> RevId {
        mkid("ummxkyyk", "b0d38b770763ab74f297f483f4b42c961647b4fb")
    }

    /// An immutable commit (parent of immutable_bookmark)
    pub fn immutable_parent() -> RevId {
        mkid("qxpuyyzu", "7c115c5a32ae0347fe63559f2869891bd5e942cd")
    }

    /// An immutable commit (grandparent of immutable_bookmark)
    pub fn immutable_grandparent() -> RevId {
        mkid("lkqlzmoy", "f74aa777a736687d1e8c4a2ea55a6821a3ef16ca")
    }

    /// A commit with a conflict in b.txt
    pub fn conflict_bookmark() -> RevId {
        mkid("nwrnuwyp", "702f1b97e561953e44ad254fa1a4e59e0e31cf16")
    }

    /// Child of conflict_bookmark that resolves the conflict
    pub fn resolve_conflict() -> RevId {
        mkid("rrxroxys", "28b2d99e4b39a7aea7277c4408427c7733f4f626")
    }

    /// Mutable commit that changed b.txt from "1" to "11"
    pub fn hunk_source() -> RevId {
        mkid("kmtstztw", "25f44e213721c7be11ac1d3f5d26bea2b7d472db")
    }

    /// Contains hunk_test.txt with 5 lines: line1-line5
    pub fn hunk_base() -> RevId {
        mkid("vkwrnurr", "6efa7f9eade075121b33679efe232dac1a612a2d")
    }

    /// Child of hunk_base, modifies line 2: line2 -> modified2
    pub fn hunk_child_single() -> RevId {
        mkid("nkrxruxq", "45835b6809b4def71e04b7823c6b3ac08a2f217d")
    }

    /// Child of hunk_base, modifies lines 2 and 4: line2 -> changed2, line4 -> changed4
    pub fn hunk_child_multi() -> RevId {
        mkid("nqwrstxx", "e2f7f467dce1ed7ff6087c01ca986e24cc039d8c")
    }

    /// Child of hunk_base, adds lines 6-8: new6, new7, new8
    pub fn hunk_sibling() -> RevId {
        mkid("xwsxmqwz", "5dd18b61b3e94d60019265a0b6e5e74dff93d482")
    }

    /// Child of hunk_child_single, modifies line 3: line3 -> grandchild3
    /// This creates a 3-level hierarchy: hunk_base -> hunk_child_single -> hunk_grandchild
    pub fn hunk_grandchild() -> RevId {
        mkid("ywskwwql", "56ec47c934036e08f99c55302d328eb8f163c74e")
    }

    /// Contains small.txt with 2 lines: line1, line2
    pub fn small_parent() -> RevId {
        mkid("nwzznmzm", "aebc9a99fb78a1717a008ed30619f55075bc65b1")
    }

    /// Child of small_parent, modifies line 2: line2 -> changed
    pub fn small_child() -> RevId {
        mkid("mvttnyym", "d18a83638ec0c7f1918f4e3ac14d07812880cab1")
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
