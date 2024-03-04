use std::{fs::File, path::PathBuf};

use tempfile::{tempdir, TempDir};
use zip::ZipArchive;

use crate::messages::RevId;

fn mkchid(id: &str) -> RevId {
    RevId {
        hex: id.to_owned(),
        prefix: id.to_owned(),
        rest: "".to_owned(),
    }
}

fn mkrepo() -> TempDir {
    let repo_dir = tempdir().unwrap();
    let mut archive_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    archive_path.push("resources/test-repo.zip");
    let archive_file = File::open(&archive_path).unwrap();
    let mut archive = ZipArchive::new(archive_file).unwrap();

    archive.extract(repo_dir.path()).unwrap();

    repo_dir
}

mod session {
    use std::{path::PathBuf, sync::mpsc::channel};

    use anyhow::Result;

    use crate::{
        gui_util::WorkerSession,
        messages::{LogPage, RepoConfig},
        worker::{Session, SessionEvent},
    };

    #[test]
    fn start_and_stop() -> Result<()> {
        let (tx, rx) = channel::<SessionEvent>();
        tx.send(SessionEvent::EndSession)?;
        WorkerSession::default().handle_events(&rx)?;
        Ok(())
    }

    #[test]
    fn load_repo() -> Result<()> {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_good_repo, rx_good_repo) = channel::<Result<RepoConfig>>();
        let (tx_bad_repo, rx_bad_repo) = channel::<Result<RepoConfig>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_good_repo,
            cwd: None,
        })?;
        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_bad_repo,
            cwd: Some(PathBuf::new()),
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx)?;

        let config = rx_good_repo.recv()??;
        assert!(matches!(config, RepoConfig::Workspace { .. }));

        let config = rx_bad_repo.recv()??;
        assert!(matches!(config, RepoConfig::NoWorkspace { .. }));

        Ok(())
    }

    #[test]
    fn reload_repo() -> Result<()> {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_first_repo, rx_first_repo) = channel::<Result<RepoConfig>>();
        let (tx_second_repo, rx_second_repo) = channel::<Result<RepoConfig>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_first_repo,
            cwd: None,
        })?;
        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_second_repo,
            cwd: None,
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
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_query, rx_query) = channel::<Result<LogPage>>();
        let (tx_reload, rx_reload) = channel::<Result<RepoConfig>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            cwd: None,
        })?;
        tx.send(SessionEvent::QueryLog {
            tx: tx_query,
            query: "none()".to_owned(),
        })?;
        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_reload,
            cwd: None,
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
    fn load_log() -> Result<()> {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
        let (tx_query, rx_query) = channel::<Result<LogPage>>();

        tx.send(SessionEvent::OpenWorkspace {
            tx: tx_load,
            cwd: None,
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

        Ok(())
    }
}

mod mutation {
    use std::fs;

    use anyhow::Result;
    use jj_lib::{backend::TreeValue, repo_path::RepoPath};

    use crate::{
        gui_util::WorkerSession,
        messages::DescribeRevision,
        worker::{mutations, queries},
    };

    use super::{mkchid, mkrepo};

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

        ws.snapshot_working_copy()?;
        assert_eq!(&old_wc, ws.wc_id());

        fs::write(repo.path().join("new.txt"), []).unwrap();

        ws.snapshot_working_copy()?;
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
    fn describe() -> Result<()> {
        let repo = mkrepo();
        let wc_str = String::from("romxvykp");
        let wc_chid = mkchid("romxvykp");

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let page = queries::LogQuery::new(wc_str.clone()).get_page(&ws)?;
        assert_eq!(1, page.rows.len());
        assert_eq!("", page.rows[0].revision.description.lines[0]);

        mutations::describe_revision(
            &mut ws,
            DescribeRevision {
                change_id: wc_chid,
                new_description: "wip".to_owned(),
            },
        )?;

        let page = queries::LogQuery::new(wc_str.clone()).get_page(&ws)?;
        assert_eq!(1, page.rows.len());
        assert_eq!("wip", page.rows[0].revision.description.lines[0]); // this passes if we recreate the WS from the directory

        Ok(())
    }

    #[test]
    fn describe_with_snapshot() -> Result<()> {
        let repo = mkrepo();
        let wc_str = String::from("romxvykp");
        let wc_chid = mkchid("romxvykp");

        let mut session = WorkerSession::default();
        let mut ws = session.load_directory(repo.path())?;

        let page = queries::LogQuery::new(wc_str.clone()).get_page(&ws)?;
        assert_eq!(1, page.rows.len());
        assert_eq!("", &page.rows[0].revision.description.lines[0]);

        let rev = queries::query_revision(&ws, &page.rows[0].revision.commit_id.hex)?;
        assert_eq!(0, rev.diff.len());

        fs::write(repo.path().join("new.txt"), []).unwrap();
        mutations::describe_revision(
            &mut ws,
            DescribeRevision {
                change_id: wc_chid,
                new_description: "wip".to_owned(),
            },
        )?;

        let page = queries::LogQuery::new(wc_str.clone()).get_page(&ws)?;
        assert_eq!(1, page.rows.len());
        assert_eq!("wip", page.rows[0].revision.description.lines[0]);

        let rev = queries::query_revision(&ws, &page.rows[0].revision.commit_id.hex)?;
        assert_ne!(0, rev.diff.len());

        Ok(())
    }
}
