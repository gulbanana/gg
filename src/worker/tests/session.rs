use super::{mkid, mkrepo, revs};
use crate::{
    messages::{LogPage, RepoConfig, RevSet, RevsResult},
    worker::{Session, SessionEvent, WorkerSession},
};
use anyhow::{Context, Result};
use assert_matches::assert_matches;
use jj_lib::config::ConfigSource;
use std::{path::PathBuf, sync::mpsc::channel};

#[tokio::test]
async fn start_and_stop() -> Result<()> {
    let (tx, rx) = channel::<SessionEvent>();
    tx.send(SessionEvent::EndSession)?;
    WorkerSession::default().handle_events(&rx).await?;
    Ok(())
}

#[tokio::test]
async fn load_repo() -> Result<()> {
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

    WorkerSession::default().handle_events(&rx).await?;

    let config = rx_good_repo.recv()??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    let config = rx_bad_repo.recv()??;
    assert!(matches!(config, RepoConfig::LoadError { .. }));

    Ok(())
}

#[tokio::test]
async fn reload_repo() -> Result<()> {
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

    WorkerSession::default().handle_events(&rx).await?;

    let config = rx_first_repo.recv()??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    let config = rx_second_repo.recv()??;
    assert!(matches!(config, RepoConfig::Workspace { .. }));

    Ok(())
}

#[tokio::test]
async fn reload_with_default_query() -> Result<()> {
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

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;
    _ = rx_query.recv()??;
    let config = rx_reload.recv()??;
    assert_matches!(config, RepoConfig::Workspace { latest_query, .. } if latest_query == "none()");

    Ok(())
}

#[tokio::test]
async fn query_log_single() -> Result<()> {
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

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;
    let page = rx_query.recv()??;
    assert_eq!(1, page.rows.len());
    assert!(!page.has_more);

    Ok(())
}

#[tokio::test]
async fn query_log_multi() -> Result<()> {
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
    .handle_events(&rx)
    .await?;

    rx_load.recv()??;

    let page1 = rx_page1.recv()??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let page2 = rx_page2.recv()??;
    assert_eq!(7, page2.rows.len());
    assert!(page2.has_more); // Still 4 more commits

    Ok(())
}

#[tokio::test]
async fn query_log_multi_restart() -> Result<()> {
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
    .handle_events(&rx)
    .await?;

    rx_load.recv()??;

    let page1 = rx_page1.recv()??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let page1b = rx_page1b.recv()??;
    assert_eq!(7, page1b.rows.len());
    assert!(page1b.has_more);

    let page2 = rx_page2.recv()??;
    assert_eq!(7, page2.rows.len());
    assert!(page2.has_more); // Still 4 more commits

    Ok(())
}

#[tokio::test]
async fn query_log_multi_interrupt() -> Result<()> {
    let repo = mkrepo();
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
    let (tx_page1, rx_page1) = channel::<Result<LogPage>>();
    let (tx_rev, rx_rev) = channel::<Result<RevsResult>>();
    let (tx_page2, rx_page2) = channel::<Result<LogPage>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::QueryLog {
        tx: tx_page1,
        query: "all()".to_owned(),
    })?;
    tx.send(SessionEvent::QueryRevisions {
        tx: tx_rev,
        set: RevSet {
            from: revs::working_copy(),
            to: revs::working_copy(),
        },
    })?;
    tx.send(SessionEvent::QueryLogNextPage { tx: tx_page2 })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession {
        force_log_page_size: Some(7),
        ..Default::default()
    }
    .handle_events(&rx)
    .await?;

    rx_load.recv()??;

    let page1 = rx_page1.recv()??;
    assert_eq!(7, page1.rows.len());
    assert!(page1.has_more);

    let rev = rx_rev.recv()??;
    assert!(
        matches!(rev, RevsResult::Detail { headers, .. } if headers.last().unwrap().is_working_copy)
    );

    let page2 = rx_page2.recv()??;
    assert_eq!(7, page2.rows.len());
    assert!(page2.has_more); // Still 4 more commits

    Ok(())
}

#[tokio::test]
async fn query_check_immutable() -> Result<()> {
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
    .handle_events(&rx)
    .await?;

    rx_load.recv()??;

    let page = rx_page.recv()??;
    assert_eq!(2, page.rows.len());
    assert!(!page.rows[0].revision.is_immutable);
    assert!(page.rows[1].revision.is_immutable);

    Ok(())
}

#[tokio::test]
async fn query_revs_not_found() -> Result<()> {
    let repo = mkrepo();

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
    let (tx_query, rx_query) = channel::<Result<RevsResult>>();

    let bad_id = mkid("abcdefghijklmnopqrstuvwxyz", "00000000");
    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::QueryRevisions {
        tx: tx_query,
        set: RevSet {
            from: bad_id.clone(),
            to: bad_id,
        },
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;
    let result = rx_query.recv()??;

    assert_matches!(result, RevsResult::NotFound { set } if set.from.change.hex == "abcdefghijklmnopqrstuvwxyz");

    Ok(())
}

#[tokio::test]
async fn config_read() -> Result<()> {
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

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;
    let result = rx_read.recv()?;

    assert!(result.is_ok()); // key may be empty, but should exist due to defaults

    Ok(())
}

#[tokio::test]
async fn config_write() -> Result<()> {
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

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;
    let result = rx_read.recv()??;

    assert_eq!(vec!["a".to_string(), "b".to_string()], result);

    Ok(())
}

#[tokio::test]
async fn init_workspace_internal() -> Result<()> {
    let dir = tempfile::tempdir()?;

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_init.recv()??;
    assert_eq!(result, dunce::canonicalize(dir.path())?);

    assert!(dir.path().join(".jj").exists());
    assert!(!dir.path().join(".git").exists());

    Ok(())
}

#[tokio::test]
async fn init_workspace_colocated() -> Result<()> {
    let dir = tempfile::tempdir()?;

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: dir.path().to_owned(),
        colocated: true,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_init.recv()??;
    assert_eq!(result, dunce::canonicalize(dir.path())?);

    // Verify both .jj and .git were created
    assert!(dir.path().join(".jj").exists());
    assert!(dir.path().join(".git").exists());

    Ok(())
}

#[tokio::test]
async fn init_workspace_colocated_existing_git() -> Result<()> {
    let dir = tempfile::tempdir()?;

    std::fs::create_dir(dir.path().join(".git"))?;
    std::fs::write(
        dir.path().join(".git").join("HEAD"),
        "ref: refs/heads/main\n",
    )?;
    std::fs::create_dir(dir.path().join(".git").join("objects"))?;
    std::fs::create_dir(dir.path().join(".git").join("refs"))?;

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: dir.path().to_owned(),
        colocated: true,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_init.recv()??;
    assert_eq!(result, dunce::canonicalize(dir.path())?);

    assert!(dir.path().join(".jj").exists());
    assert!(dir.path().join(".git").exists());

    Ok(())
}

#[tokio::test]
async fn init_workspace_from_workspace_session() -> Result<()> {
    let existing_repo = mkrepo();
    let new_dir = tempfile::tempdir()?;

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(existing_repo.path().to_owned()),
    })?;
    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: new_dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;

    let result = rx_init.recv()??;
    assert_eq!(result, dunce::canonicalize(new_dir.path())?);
    assert!(new_dir.path().join(".jj").exists());

    Ok(())
}

#[tokio::test]
async fn init_workspace_already_exists() -> Result<()> {
    let dir = tempfile::tempdir()?;

    std::fs::create_dir(dir.path().join(".jj"))?;

    let (tx, rx) = channel::<SessionEvent>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_init.recv()?;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));

    Ok(())
}

#[tokio::test]
async fn clone_workspace_internal() -> Result<()> {
    // Create a source repo to clone from (colocated so it has .git)
    let source_dir = tempfile::tempdir()?;
    let dest_dir = tempfile::tempdir()?;

    // First, init a colocated source repo
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: source_dir.path().to_owned(),
        colocated: true,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;
    rx_init.recv()??;

    // Now clone from the source repo
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_clone, rx_clone) = channel::<Result<PathBuf>>();

    let source_path = source_dir.path().to_string_lossy().to_string();
    tx.send(SessionEvent::CloneWorkspace {
        tx: tx_clone,
        source_url: source_path,
        wd: dest_dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_clone.recv()??;
    assert_eq!(result, dunce::canonicalize(dest_dir.path())?);

    // Verify .jj was created but not .git (internal clone)
    assert!(dest_dir.path().join(".jj").exists());
    assert!(!dest_dir.path().join(".git").exists());

    Ok(())
}

#[tokio::test]
async fn clone_workspace_colocated() -> Result<()> {
    // Create a source repo to clone from
    let source_dir = tempfile::tempdir()?;
    let dest_dir = tempfile::tempdir()?;

    // First, init a colocated source repo
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_init, rx_init) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::InitWorkspace {
        tx: tx_init,
        wd: source_dir.path().to_owned(),
        colocated: true,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;
    rx_init.recv()??;

    // Now clone from the source repo with colocated option
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_clone, rx_clone) = channel::<Result<PathBuf>>();

    let source_path = source_dir.path().to_string_lossy().to_string();
    tx.send(SessionEvent::CloneWorkspace {
        tx: tx_clone,
        source_url: source_path,
        wd: dest_dir.path().to_owned(),
        colocated: true,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_clone.recv()??;
    assert_eq!(result, dunce::canonicalize(dest_dir.path())?);

    // Verify both .jj and .git were created
    assert!(dest_dir.path().join(".jj").exists());
    assert!(dest_dir.path().join(".git").exists());

    Ok(())
}

#[tokio::test]
async fn clone_workspace_from_workspace_session() -> Result<()> {
    let existing_repo = mkrepo();
    let source_dir = tempfile::tempdir()?;
    let dest_dir = tempfile::tempdir()?;

    // First, init a source repo to clone from
    {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_init, rx_init) = channel::<Result<PathBuf>>();

        tx.send(SessionEvent::InitWorkspace {
            tx: tx_init,
            wd: source_dir.path().to_owned(),
            colocated: true,
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx).await?;
        rx_init.recv()??;
    }

    // Now open an existing workspace and clone from there
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_load, rx_load) = channel::<Result<RepoConfig>>();
    let (tx_clone, rx_clone) = channel::<Result<PathBuf>>();

    tx.send(SessionEvent::OpenWorkspace {
        tx: tx_load,
        wd: Some(existing_repo.path().to_owned()),
    })?;

    let source_path = source_dir.path().to_string_lossy().to_string();
    tx.send(SessionEvent::CloneWorkspace {
        tx: tx_clone,
        source_url: source_path,
        wd: dest_dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    _ = rx_load.recv()??;

    let result = rx_clone.recv()??;
    assert_eq!(result, dunce::canonicalize(dest_dir.path())?);
    assert!(dest_dir.path().join(".jj").exists());

    Ok(())
}

#[tokio::test]
async fn clone_workspace_dest_nonempty_error() -> Result<()> {
    let source_dir = tempfile::tempdir()?;
    let dest_dir = tempfile::tempdir()?;

    // First, init a source repo
    {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_init, rx_init) = channel::<Result<PathBuf>>();

        tx.send(SessionEvent::InitWorkspace {
            tx: tx_init,
            wd: source_dir.path().to_owned(),
            colocated: true,
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx).await?;
        rx_init.recv()??;
    }

    // Create a file in destination to make it non-empty
    std::fs::write(dest_dir.path().join("existing_file.txt"), "content")?;

    // Try to clone - should fail
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_clone, rx_clone) = channel::<Result<PathBuf>>();

    let source_path = source_dir.path().to_string_lossy().to_string();
    tx.send(SessionEvent::CloneWorkspace {
        tx: tx_clone,
        source_url: source_path,
        wd: dest_dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_clone.recv()?;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not empty"));

    Ok(())
}

#[tokio::test]
async fn clone_workspace_dest_empty_ok() -> Result<()> {
    let source_dir = tempfile::tempdir()?;
    let dest_dir = tempfile::tempdir()?;

    // First, init a source repo
    {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_init, rx_init) = channel::<Result<PathBuf>>();

        tx.send(SessionEvent::InitWorkspace {
            tx: tx_init,
            wd: source_dir.path().to_owned(),
            colocated: true,
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx).await?;
        rx_init.recv()??;
    }

    // Destination exists but is empty - should succeed
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_clone, rx_clone) = channel::<Result<PathBuf>>();

    let source_path = source_dir.path().to_string_lossy().to_string();
    tx.send(SessionEvent::CloneWorkspace {
        tx: tx_clone,
        source_url: source_path,
        wd: dest_dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_clone.recv()??;
    assert_eq!(result, dunce::canonicalize(dest_dir.path())?);
    assert!(dest_dir.path().join(".jj").exists());

    Ok(())
}

#[cfg(windows)]
const EOL: &'static str = "\r\n";
#[cfg(not(windows))]
const EOL: &'static str = "\n";

#[tokio::test]
async fn clone_workspace_checks_out_file_content() -> Result<()> {
    use std::process::{Command, Stdio};

    let source_dir = tempfile::tempdir()?;
    let dest_dir = tempfile::tempdir()?;

    // Init a colocated source repo
    {
        let (tx, rx) = channel::<SessionEvent>();
        let (tx_init, rx_init) = channel::<Result<PathBuf>>();

        tx.send(SessionEvent::InitWorkspace {
            tx: tx_init,
            wd: source_dir.path().to_owned(),
            colocated: true,
        })?;
        tx.send(SessionEvent::EndSession)?;

        WorkerSession::default().handle_events(&rx).await?;
        rx_init.recv()??;
    }

    // Create a file with content in the source repo
    let mut test_content = String::from("Hello from test repository!");
    test_content += EOL;
    test_content += "Line 2\r\n";
    std::fs::write(source_dir.path().join("test_file.txt"), &test_content)?;

    // Use git commands to add and commit the file
    let add_status = Command::new("git")
        .args(["add", "test_file.txt"])
        .current_dir(source_dir.path())
        .status()
        .context("Failed to run git add")?;
    assert!(add_status.success(), "git add should succeed");

    let commit_status = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@test.com",
            "commit",
            "-m",
            "Add test file",
        ])
        .current_dir(source_dir.path())
        .stdout(Stdio::null())
        .status()
        .context("Failed to run git commit")?;
    assert!(commit_status.success(), "git commit should succeed");

    // Clone from the source repo
    let (tx, rx) = channel::<SessionEvent>();
    let (tx_clone, rx_clone) = channel::<Result<PathBuf>>();

    let source_path = source_dir.path().to_string_lossy().to_string();
    tx.send(SessionEvent::CloneWorkspace {
        tx: tx_clone,
        source_url: source_path,
        wd: dest_dir.path().to_owned(),
        colocated: false,
    })?;
    tx.send(SessionEvent::EndSession)?;

    WorkerSession::default().handle_events(&rx).await?;

    let result = rx_clone.recv()??;
    assert_eq!(result, dunce::canonicalize(dest_dir.path())?);

    // Verify the file content was checked out
    let opened_file = dest_dir.path().join("test_file.txt");
    assert!(
        opened_file.exists(),
        "test_file.txt should exist in cloned repo"
    );

    let opened_content = std::fs::read_to_string(&opened_file)?;
    assert_eq!(opened_content, test_content, "File content should match");

    Ok(())
}
