use super::{mkid, mkrepo, revs};
use crate::{
    messages::{LogPage, RepoConfig, RevResult},
    worker::{Session, SessionEvent, WorkerSession},
};
use anyhow::Result;
use jj_lib::config::ConfigSource;
use std::{path::PathBuf, sync::mpsc::channel};

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
    assert!(!page.has_more);

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
        force_log_page_size: Some(7),
        ..Default::default()
    }
    .handle_events(&rx)?;

    rx_load.recv()??;

    let page1 = rx_page1.recv()??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let page2 = rx_page2.recv()??;
    assert_eq!(5, page2.rows.len());
    assert!(!page2.has_more);

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
        force_log_page_size: Some(7),
        ..Default::default()
    }
    .handle_events(&rx)?;

    rx_load.recv()??;

    let page1 = rx_page1.recv()??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let page1b = rx_page1b.recv()??;
    assert_eq!(7, page1b.rows.len());
    assert!(page1b.has_more);

    let page2 = rx_page2.recv()??;
    assert_eq!(5, page2.rows.len());
    assert!(!page2.has_more);

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
        force_log_page_size: Some(7),
        ..Default::default()
    }
    .handle_events(&rx)?;

    rx_load.recv()??;

    let page1 = rx_page1.recv()??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let rev = rx_rev.recv()??;
    assert!(matches!(rev, RevResult::Detail { header, .. } if header.is_working_copy));

    let page2 = rx_page2.recv()??;
    assert_eq!(5, page2.rows.len());
    assert!(!page2.has_more);

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

#[test]
fn config_read() -> Result<()> {
    let repo = mkrepo();

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
    let (tx_read, rx_read) = channel::<Result<Vec<String>>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::ReadConfigArray {
        tx: tx_read,
        key: vec!["gg".into(), "ui".into(), "recent-workspaces".into()],
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx)?;

    _ = rx_load.recv()??;
    let result = rx_read.recv()?;

    assert!(result.is_ok()); // key may be empty, but should exist due to defaults

    Ok(())
}

#[test]
fn config_write() -> Result<()> {
    let repo = mkrepo();

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
    let (tx_read, rx_read) = channel::<Result<Vec<String>>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::WriteConfigArray {
        scope: ConfigSource::Repo,
        key: vec!["gg".into(), "test".into()],
        values: vec!["a".into(), "b".into()],
    })?;
    tx.send(SessionEvent::ReadConfigArray {
        tx: tx_read,
        key: vec!["gg".into(), "test".into()],
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx)?;

    _ = rx_load.recv()??;
    let result = rx_read.recv()??;

    assert_eq!(vec!["a".to_string(), "b".to_string()], result);

    Ok(())
}
