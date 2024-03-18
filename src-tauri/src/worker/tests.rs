use std::{fs::File, path::PathBuf};

use tempfile::{tempdir, TempDir};
use zip::ZipArchive;

use crate::messages::{ChangeId, CommitId, RevId};

fn mkrepo() -> TempDir {
    let repo_dir = tempdir().unwrap();
    let mut archive_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    archive_path.push("resources/test-repo.zip");
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

mod revs {
    use crate::messages::RevId;

    use super::mkid;

    pub fn working_copy() -> RevId {
        mkid("kppkuplp", "a625ed5aa71d7c68b98b1d1521b2e1cbd0c54a0d")
    }

    pub fn main_branch() -> RevId {
        mkid("mnkoropy", "87e9c6c03e1b727ff712d962c03b32fffb704bc0")
    }

    pub fn conflict_branch() -> RevId {
        mkid("nwrnuwyp", "880abeefdd3ac344e2a0901c5f486d02d34053da")
    }

    pub fn resolve_conflict() -> RevId {
        mkid("rrxroxys", "db297552443bcafc0f0715b7ace7fb4488d7954d")
    }
}

mod session {
    use std::{path::PathBuf, sync::mpsc::channel};

    use anyhow::Result;

    use crate::{
        messages::{LogPage, RepoConfig, RevResult},
        worker::{Session, SessionEvent, WorkerSession},
    };

    use super::{mkid, mkrepo, revs};

    #[test]
    fn start_and_stop() -> Result<()> {
        let (tx, rx) = channel::<SessionEvent>();
        tx.send(SessionEvent::EndSession)?;
        WorkerSession::default().handle_events(&rx)?;
        Ok(())
    }

    #[test]
    fn load_repo() -> Result<()> {
        let repo = mkrepo();

        let (tx, rx) = channel::<SessionEvent>();
        let (tx_good_repo, rx_good_repo) = channel::<Result<RepoConfig>>();
        let (tx_bad_repo, rx_bad_repo) = channel::<Result<RepoConfig>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_good_repo,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_bad_repo,
            wd: Some(PathBuf::new()),
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx)?;

        let config = rx_good_repo.recv()??;
        assert!(matches!(config, RepoConfig::Workspace { .. }));

        let config = rx_bad_repo.recv()??;
        assert!(matches!(config, RepoConfig::LoadError { .. }));

        Ok(())
    }

    #[test]
    fn reload_repo() -> Result<()> {
        let repo1 = mkrepo();
        let repo2 = mkrepo();

        let (tx, rx) = channel::<SessionEvent>();
        let (tx_first_repo, rx_first_repo) = channel::<Result<RepoConfig>>();
        let (tx_second_repo, rx_second_repo) = channel::<Result<RepoConfig>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_first_repo,
            wd: Some(repo1.path().to_owned()),
        })?;
        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_second_repo,
            wd: Some(repo2.path().to_owned()),
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx)?;

        let config = rx_first_repo.recv()??;
        assert!(matches!(config, RepoConfig::Workspace { .. }));

        let config = rx_second_repo.recv()??;
        assert!(matches!(config, RepoConfig::Workspace { .. }));

        Ok(())
    }

    #[test]
    fn reload_with_default_query() -> Result<()> {
        let repo = mkrepo();

        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_query, rx_query) = channel::<Result<LogPage>>();
        let (tx_reload, rx_reload) = channel::<Result<RepoConfig>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_query,
            query: "none()".to_owned(),
        })?;
        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_reload,
            wd: None,
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx)?;

        _ = rx_load.recv()??;
        _ = rx_query.recv()??;
        let config = rx_reload.recv()??;
        assert!(
            matches!(config, RepoConfig::Workspace { latest_query, .. } if latest_query == "none()")
        );

        Ok(())
    }

    #[test]
    fn query_log_single() -> Result<()> {
        let repo = mkrepo();

        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_query, rx_query) = channel::<Result<LogPage>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_query,
            query: "@".to_owned(),
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx)?;

        _ = rx_load.recv()??;
        let page = rx_query.recv()??;
        assert_eq!(1, page.rows.len());
        assert_eq!(false, page.has_more);

        Ok(())
    }

    #[test]
    fn query_log_multi() -> Result<()> {
        let repo = mkrepo();
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_page1, rx_page1) = channel::<Result<LogPage>>();
        let (tx_page2, rx_page2) = channel::<Result<LogPage>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_page1,
            query: "all()".to_owned(),
        })?;
        tx.send(SessionEvent::QueryLogNextPage { tx: tx_page2 })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession {
            force_log_page_size: Some(6),
            ..Default::default()
        }
        .handle_events(&rx)?;

        rx_load.recv()??;

        let page1 = rx_page1.recv()??;
        assert_eq!(6, page1.rows.len());
        assert_eq!(true, page1.has_more);

        let page2 = rx_page2.recv()??;
        assert_eq!(5, page2.rows.len());
        assert_eq!(false, page2.has_more);

        Ok(())
    }

    #[test]
    fn query_log_multi_restart() -> Result<()> {
        let repo = mkrepo();
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_page1, rx_page1) = channel::<Result<LogPage>>();
        let (tx_page1b, rx_page1b) = channel::<Result<LogPage>>();
        let (tx_page2, rx_page2) = channel::<Result<LogPage>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_page1,
            query: "all()".to_owned(),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_page1b,
            query: "all()".to_owned(),
        })?;
        tx.send(SessionEvent::QueryLogNextPage { tx: tx_page2 })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession {
            force_log_page_size: Some(6),
            ..Default::default()
        }
        .handle_events(&rx)?;

        rx_load.recv()??;

        let page1 = rx_page1.recv()??;
        assert_eq!(6, page1.rows.len());
        assert_eq!(true, page1.has_more);

        let page1b = rx_page1b.recv()??;
        assert_eq!(6, page1b.rows.len());
        assert_eq!(true, page1b.has_more);

        let page2 = rx_page2.recv()??;
        assert_eq!(5, page2.rows.len());
        assert_eq!(false, page2.has_more);

        Ok(())
    }

    #[test]
    fn query_log_multi_interrupt() -> Result<()> {
        let repo = mkrepo();
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_page1, rx_page1) = channel::<Result<LogPage>>();
        let (tx_rev, rx_rev) = channel::<Result<RevResult>>();
        let (tx_page2, rx_page2) = channel::<Result<LogPage>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_page1,
            query: "all()".to_owned(),
        })?;
        tx.send(SessionEvent::QueryRevision {
            tx: tx_rev,
            id: revs::working_copy(),
        })?;
        tx.send(SessionEvent::QueryLogNextPage { tx: tx_page2 })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession {
            force_log_page_size: Some(6),
            ..Default::default()
        }
        .handle_events(&rx)?;

        rx_load.recv()??;

        let page1 = rx_page1.recv()??;
        assert_eq!(6, page1.rows.len());
        assert_eq!(true, page1.has_more);

        let rev = rx_rev.recv()??;
        assert!(matches!(rev, RevResult::Detail { header, .. } if header.is_working_copy));

        let page2 = rx_page2.recv()??;
        assert_eq!(5, page2.rows.len());
        assert_eq!(false, page2.has_more);

        Ok(())
    }

    #[test]
    fn query_check_immutable() -> Result<()> {
        let repo = mkrepo();
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_page, rx_page) = channel::<Result<LogPage>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_page,
            query: "@|main@origin".to_owned(),
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession {
            force_log_page_size: Some(2),
            ..Default::default()
        }
        .handle_events(&rx)?;

        rx_load.recv()??;

        let page = rx_page.recv()??;
        assert_eq!(2, page.rows.len());
        assert!(!page.rows[0].revision.is_immutable);
        assert!(page.rows[1].revision.is_immutable);

        Ok(())
    }

    #[test]
    fn query_rev_not_found() -> Result<()> {
        let repo = mkrepo();

        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_query, rx_query) = channel::<Result<RevResult>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            wd: Some(repo.path().to_owned()),
        })?;
        tx.send(SessionEvent::QueryRevision {
            tx: tx_query,
            id: mkid("abcdefghijklmnopqrstuvwxyz", "00000000"),
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx)?;

        _ = rx_load.recv()??;
        let result = rx_query.recv()??;

        assert!(
            matches!(result, RevResult::NotFound { id } if id.change.hex == "abcdefghijklmnopqrstuvwxyz")
        );

        Ok(())
    }
}

mod mutation {
    use std::fs;

    use anyhow::Result;
    use jj_lib::{backend::TreeValue, repo_path::RepoPath};

    use crate::{
        messages::{
            CheckoutRevision, CreateRevision, DescribeRevision, MoveChanges, MutationResult,
            RevResult, TreePath,
        },
        worker::{queries, Mutation, WorkerSession},
    };

    use super::{mkrepo, revs};

    #[test]
    fn wc_path_is_visible() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        let commit = ws.get_commit(ws.wc_id())?;
        let value = commit
            .tree()?
            .path_value(RepoPath::from_internal_string("a.txt"));

        assert!(value.is_resolved());
        assert!(value
            .first()
            .as_ref()
            .is_some_and(|x| matches!(x, TreeValue::File { .. })));

        Ok(())
    }

    #[test]
    fn snapshot_updates_wc_if_changed() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;
        let old_wc = ws.wc_id().clone();

        assert!(!ws.import_and_snapshot(true)?);
        assert_eq!(&old_wc, ws.wc_id());

        fs::write(repo.path().join("new.txt"), []).unwrap();

        assert!(ws.import_and_snapshot(true)?);
        assert_ne!(&old_wc, ws.wc_id());

        Ok(())
    }

    #[test]
    fn transaction_updates_wc_if_snapshot() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;
        let old_wc = ws.wc_id().clone();

        fs::write(repo.path().join("new.txt"), []).unwrap();

        let tx = ws.start_transaction()?;
        ws.finish_transaction(tx, "do nothing")?;

        assert_ne!(&old_wc, ws.wc_id());

        Ok(())
    }

    #[test]
    fn transaction_snapshot_path_is_visible() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        fs::write(repo.path().join("new.txt"), []).unwrap();

        let tx = ws.start_transaction()?;
        ws.finish_transaction(tx, "do nothing")?;

        let commit = ws.get_commit(ws.wc_id())?;
        let value = commit
            .tree()?
            .path_value(RepoPath::from_internal_string("new.txt"));

        assert!(value.is_resolved());
        assert!(value
            .first()
            .as_ref()
            .is_some_and(|x| matches!(x, TreeValue::File { .. })));

        Ok(())
    }

    #[test]
    fn edit() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let head_rev = queries::query_revision(&ws, revs::working_copy())?;
        let conflict_rev = queries::query_revision(&ws, revs::conflict_branch())?;
        assert!(matches!(head_rev, RevResult::Detail { header, .. } if header.is_working_copy));
        assert!(
            matches!(conflict_rev, RevResult::Detail { header, .. } if !header.is_working_copy)
        );

        let result = CheckoutRevision {
            id: revs::conflict_branch(),
        }
        .execute_unboxed(&mut ws)?;
        assert!(matches!(result, MutationResult::UpdatedSelection { .. }));

        let head_rev = queries::query_revision(&ws, revs::working_copy())?;
        let conflict_rev = queries::query_revision(&ws, revs::conflict_branch())?;
        assert!(matches!(head_rev, RevResult::NotFound { .. }));
        assert!(matches!(conflict_rev, RevResult::Detail { header, .. } if header.is_working_copy));

        Ok(())
    }

    #[test]
    fn new_single_parent() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let parent_rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(matches!(parent_rev, RevResult::Detail { header, .. } if header.is_working_copy));

        let result = CreateRevision {
            parent_ids: vec![revs::working_copy()],
        }
        .execute_unboxed(&mut ws)?;

        match result {
            MutationResult::UpdatedSelection { new_selection, .. } => {
                let parent_rev = queries::query_revision(&ws, revs::working_copy())?;
                let child_rev = queries::query_revision(&ws, new_selection.id)?;
                assert!(
                    matches!(parent_rev, RevResult::Detail { header, .. } if !header.is_working_copy)
                );
                assert!(
                    matches!(child_rev, RevResult::Detail { header, .. } if header.is_working_copy)
                );
            }
            _ => assert!(false, "CreateRevision failed"),
        }

        Ok(())
    }

    #[test]
    fn new_multi_parent() -> Result<()> {
        let repo: tempfile::TempDir = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let parent_rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(matches!(parent_rev, RevResult::Detail { header, .. } if header.is_working_copy));

        let result = CreateRevision {
            parent_ids: vec![revs::working_copy(), revs::conflict_branch()],
        }
        .execute_unboxed(&mut ws)?;

        match result {
            MutationResult::UpdatedSelection { new_selection, .. } => {
                let child_rev = queries::query_revision(&ws, new_selection.id)?;
                assert!(
                    matches!(child_rev, RevResult::Detail { parents, .. } if parents.len() == 2)
                );
            }
            _ => assert!(false, "CreateRevision failed"),
        }

        Ok(())
    }

    #[test]
    fn describe() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(
            matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0] == "")
        );

        let result = DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)?;
        assert!(matches!(result, MutationResult::Updated { .. }));

        let rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(
            matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0] == "wip")
        );

        Ok(())
    }

    #[test]
    fn describe_with_snapshot() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(
            matches!(rev, RevResult::Detail { header, changes, .. } if header.description.lines[0] == "" && changes.len() == 0)
        );

        fs::write(repo.path().join("new.txt"), []).unwrap(); // changes the WC commit

        DescribeRevision {
            id: revs::working_copy(),
            new_description: "wip".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)?;

        let rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(
            matches!(rev, RevResult::Detail { header, changes, .. } if header.description.lines[0] == "wip" && changes.len() != 0)
        );

        Ok(())
    }

    #[test]
    fn move_changes() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let parent_rev = queries::query_revision(&ws, revs::conflict_branch())?;
        assert!(matches!(parent_rev, RevResult::Detail { header, .. } if header.has_conflict));

        let result = MoveChanges {
            from_id: revs::resolve_conflict(),
            to_id: revs::conflict_branch().commit,
            paths: vec![],
        }
        .execute_unboxed(&mut ws)?;
        assert!(matches!(result, MutationResult::Updated { .. }));

        let parent_rev = queries::query_revision(&ws, revs::conflict_branch())?;
        assert!(matches!(parent_rev, RevResult::Detail { header, .. } if !header.has_conflict));

        Ok(())
    }

    #[test]
    fn move_changes_single_path() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let from_rev = queries::query_revision(&ws, revs::main_branch())?;
        let to_rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(matches!(from_rev, RevResult::Detail { changes, .. } if changes.len() == 2));
        assert!(matches!(to_rev, RevResult::Detail { changes, .. } if changes.len() == 0));

        let result = MoveChanges {
            from_id: revs::main_branch(),
            to_id: revs::working_copy().commit,
            paths: vec![TreePath {
                repo_path: "c.txt".to_owned(),
                relative_path: "".into(),
            }],
        }
        .execute_unboxed(&mut ws)?;
        assert!(matches!(result, MutationResult::Updated { .. }));

        let from_rev = queries::query_revision(&ws, revs::main_branch())?;
        let to_rev = queries::query_revision(&ws, revs::working_copy())?;
        assert!(matches!(from_rev, RevResult::Detail { changes, .. } if changes.len() == 1));
        assert!(matches!(to_rev, RevResult::Detail { changes, .. } if changes.len() == 1));

        Ok(())
    }
}
