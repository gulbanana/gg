use std::{path::PathBuf, sync::mpsc::channel};

use anyhow::Result;

use crate::{
    messages::RepoConfig,
    worker::{self, SessionEvent},
};

#[test]
fn start_and_stop() -> Result<()> {
    let (tx, rx) = channel::<SessionEvent>();
    tx.send(SessionEvent::EndSession)?;
    worker::state_main(&rx)?;
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

    worker::state_main(&rx)?;

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

    worker::state_main(&rx)?;

    let config = rx_first_repo.recv()??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    let config = rx_second_repo.recv()??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    Ok(())
}
