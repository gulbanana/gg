use super::{mkid, mkrepo, revs};
use crate::{
    messages::{LogPage, RepoConfig, RevResult},
    worker::{Session, SessionEvent, WorkerSession},
};
use anyhow::Result;
use jj_lib::config::ConfigSource;
use pollster::block_on;
use std::path::PathBuf;
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn start_and_stop() -> Result<()> {
    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    tx.send(SessionEvent::EndSession)?;
    block_on(WorkerSession::default().handle_events(&mut rx))?;
    Ok(())
}

#[test]
fn load_repo() -> Result<()> {
    let repo = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_good_repo, mut rx_good_repo) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_bad_repo, mut rx_bad_repo) = unbounded_channel::<Result<RepoConfig>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_good_repo,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_bad_repo,
        wd: Some(PathBuf::new()),
    })?;
    tx.send(SessionEvent::EndSession)?;

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    let config =
        block_on(rx_good_repo.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    let config =
        block_on(rx_bad_repo.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert!(matches!(config, RepoConfig::LoadError { .. }));

    Ok(())
}

#[test]
fn reload_repo() -> Result<()> {
    let repo1 = mkrepo();
    let repo2 = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_first_repo, mut rx_first_repo) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_second_repo, mut rx_second_repo) = unbounded_channel::<Result<RepoConfig>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_first_repo,
        wd: Some(repo1.path().to_owned()),
    })?;
    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_second_repo,
        wd: Some(repo2.path().to_owned()),
    })?;
    tx.send(SessionEvent::EndSession)?;

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    let config =
        block_on(rx_first_repo.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    let config =
        block_on(rx_second_repo.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    Ok(())
}

#[test]
fn reload_with_default_query() -> Result<()> {
    let repo = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_query, mut rx_query) = unbounded_channel::<Result<LogPage>>();
    let (tx_reload, mut rx_reload) = unbounded_channel::<Result<RepoConfig>>();

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

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    _ = block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    _ = block_on(rx_query.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    let config = block_on(rx_reload.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert!(
        matches!(config, RepoConfig::Workspace { latest_query, .. } if latest_query == "none()")
    );

    Ok(())
}

#[test]
fn query_log_single() -> Result<()> {
    let repo = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_query, mut rx_query) = unbounded_channel::<Result<LogPage>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::QueryLog {
        tx: tx_query,
        query: "@".to_owned(),
    })?;
    tx.send(SessionEvent::EndSession)?;

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    _ = block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    let page = block_on(rx_query.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(1, page.rows.len());
    assert!(!page.has_more);

    Ok(())
}

#[test]
fn query_log_multi() -> Result<()> {
    let repo = mkrepo();
    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_page1, mut rx_page1) = unbounded_channel::<Result<LogPage>>();
    let (tx_page2, mut rx_page2) = unbounded_channel::<Result<LogPage>>();

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

    block_on(
        WorkerSession {
            force_log_page_size: Some(7),
            ..Default::default()
        }
        .handle_events(&mut rx),
    )?;

    block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;

    let page1 = block_on(rx_page1.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let page2 = block_on(rx_page2.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(5, page2.rows.len());
    assert!(!page2.has_more);

    Ok(())
}

#[test]
fn query_log_multi_restart() -> Result<()> {
    let repo = mkrepo();
    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_page1, mut rx_page1) = unbounded_channel::<Result<LogPage>>();
    let (tx_page1b, mut rx_page1b) = unbounded_channel::<Result<LogPage>>();
    let (tx_page2, mut rx_page2) = unbounded_channel::<Result<LogPage>>();

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

    block_on(
        WorkerSession {
            force_log_page_size: Some(7),
            ..Default::default()
        }
        .handle_events(&mut rx),
    )?;

    block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;

    let page1 = block_on(rx_page1.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let page1b = block_on(rx_page1b.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(7, page1b.rows.len());
    assert!(page1b.has_more);

    let page2 = block_on(rx_page2.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(5, page2.rows.len());
    assert!(!page2.has_more);

    Ok(())
}

#[test]
fn query_log_multi_interrupt() -> Result<()> {
    let repo = mkrepo();
    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_page1, mut rx_page1) = unbounded_channel::<Result<LogPage>>();
    let (tx_rev, mut rx_rev) = unbounded_channel::<Result<RevResult>>();
    let (tx_page2, mut rx_page2) = unbounded_channel::<Result<LogPage>>();

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

    block_on(
        WorkerSession {
            force_log_page_size: Some(7),
            ..Default::default()
        }
        .handle_events(&mut rx),
    )?;

    block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;

    let page1 = block_on(rx_page1.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let rev = block_on(rx_rev.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert!(matches!(rev, RevResult::Detail { header, .. } if header.is_working_copy));

    let page2 = block_on(rx_page2.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(5, page2.rows.len());
    assert!(!page2.has_more);

    Ok(())
}

#[test]
fn query_check_immutable() -> Result<()> {
    let repo = mkrepo();
    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_page, mut rx_page) = unbounded_channel::<Result<LogPage>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::QueryLog {
        tx: tx_page,
        query: "@|main@origin".to_owned(),
    })?;
    tx.send(SessionEvent::EndSession)?;

    block_on(
        WorkerSession {
            force_log_page_size: Some(2),
            ..Default::default()
        }
        .handle_events(&mut rx),
    )?;

    block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;

    let page = block_on(rx_page.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    assert_eq!(2, page.rows.len());
    assert!(!page.rows[0].revision.is_immutable);
    assert!(page.rows[1].revision.is_immutable);

    Ok(())
}

#[test]
fn query_rev_not_found() -> Result<()> {
    let repo = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_query, mut rx_query) = unbounded_channel::<Result<RevResult>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::QueryRevision {
        tx: tx_query,
        id: mkid("abcdefghijklmnopqrstuvwxyz", "00000000"),
    })?;
    tx.send(SessionEvent::EndSession)?;

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    _ = block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    let result = block_on(rx_query.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;

    assert!(
        matches!(result, RevResult::NotFound { id } if id.change.hex == "abcdefghijklmnopqrstuvwxyz")
    );

    Ok(())
}

#[test]
fn config_read() -> Result<()> {
    let repo = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_read, mut rx_read) = unbounded_channel::<Result<Vec<String>>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::ReadConfigArray {
        tx: tx_read,
        key: vec!["gg".into(), "ui".into(), "recent-workspaces".into()],
    })?;
    tx.send(SessionEvent::EndSession)?;

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    _ = block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    let result = block_on(rx_read.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))?;

    assert!(result.is_ok()); // key may be empty, but should exist due to defaults

    Ok(())
}

#[test]
fn config_write() -> Result<()> {
    let repo = mkrepo();

    let (tx, mut rx) = unbounded_channel::<SessionEvent>();
    let (tx_load, mut rx_load) = unbounded_channel::<Result<RepoConfig>>();
    let (tx_read, mut rx_read) = unbounded_channel::<Result<Vec<String>>>();

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

    block_on(WorkerSession::default().handle_events(&mut rx))?;

    _ = block_on(rx_load.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;
    let result = block_on(rx_read.recv()).ok_or_else(|| anyhow::anyhow!("channel closed"))??;

    assert_eq!(vec!["a".to_string(), "b".to_string()], result);

    Ok(())
}
