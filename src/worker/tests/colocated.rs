//! colocated workspaces have a .git of their own - a directory for the main workspace of a
//! colocated repo, or a gitlink file for a workspace backed by a git worktree

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use jj_lib::{
    backend::CommitId,
    config::{ConfigLayer, ConfigSource},
    git,
    object_id::ObjectId as _,
    op_store::RefTarget,
    ref_name::RefName,
    repo::Repo as _,
};
use tempfile::{TempDir, tempdir};

use crate::{
    messages::mutations::ForgetWorkspace,
    worker::{Mutation as _, WorkerSession, WorkspaceSession},
};

async fn mkcolocated() -> Result<TempDir> {
    let dir = tempdir()?;
    WorkerSession::default()
        .init_repository(&dir.path().to_owned(), true)
        .await?;
    Ok(dir)
}

/// a destination for another workspace, outside the main one. canonical, like a path picked in
/// the UI - macOS temp dirs are reached through a symlink
fn mkdest(name: &str) -> Result<(TempDir, PathBuf)> {
    let dir = tempdir()?;
    let path = dunce::canonicalize(dir.path())?.join(name);
    Ok((dir, path))
}

/// snapshots a new file into the working-copy commit, then starts a new one on top of it
async fn commit_file(ws: &mut WorkspaceSession<'_>, file: &str) -> Result<CommitId> {
    fs::write(ws.workspace.workspace_root().join(file), file)?;
    let mut tx = ws.start_transaction().await?;
    let committed = tx.repo().store().get_commit(ws.wc_id())?;
    tx.repo_mut()
        .check_out(ws.name().to_owned(), &committed)
        .await?;
    ws.finish_transaction(tx, "commit file").await?;
    Ok(committed.id().clone())
}

fn git_head(ws: &WorkspaceSession, workdir: &Path) -> Result<Option<String>> {
    let git_repo = git::get_git_backend(ws.repo().store())?.open_git_repo_at_workdir(workdir)?;
    Ok(git_repo.head_id().ok().map(|id| id.to_string()))
}

fn git_worktree_count(repo_path: &Path) -> Result<usize> {
    let worktrees = repo_path.join(".git").join("worktrees");
    if !worktrees.exists() {
        return Ok(0);
    }
    Ok(fs::read_dir(worktrees)?.count())
}

#[tokio::test]
async fn colocated_workspace_exports_to_git() -> Result<()> {
    let repo = mkcolocated().await?;

    let mut session = WorkerSession::default();
    let mut ws = session.load_workspace(repo.path()).await?;

    let committed = commit_file(&mut ws, "a.txt").await?;
    let mut tx = ws.start_transaction().await?;
    tx.repo_mut().set_local_bookmark_target(
        RefName::new("feature"),
        RefTarget::normal(committed.clone()),
    );
    ws.finish_transaction(tx, "create bookmark").await?;

    assert_eq!(git_head(&ws, repo.path())?, Some(committed.hex()));
    let git_repo = git::get_git_backend(ws.repo().store())?.git_repo();
    let mut feature = git_repo.find_reference("refs/heads/feature")?;
    assert_eq!(feature.peel_to_id()?.to_string(), committed.hex());

    Ok(())
}

#[tokio::test]
async fn colocated_workspace_imports_from_git() -> Result<()> {
    let repo = mkcolocated().await?;

    let mut session = WorkerSession::default();
    let mut ws = session.load_workspace(repo.path()).await?;
    let committed = commit_file(&mut ws, "a.txt").await?;

    // as if `git branch` had been run behind our back
    fs::write(
        repo.path()
            .join(".git")
            .join("refs")
            .join("heads")
            .join("from-git"),
        format!("{}\n", committed.hex()),
    )?;
    ws.import_and_snapshot(true, false).await?;

    assert_eq!(
        ws.view().get_local_bookmark(RefName::new("from-git")),
        &RefTarget::normal(committed)
    );

    Ok(())
}

#[tokio::test]
async fn add_workspace_creates_worktree() -> Result<()> {
    let repo = mkcolocated().await?;
    let (_second_dir, second_path) = mkdest("second")?;

    let mut session = WorkerSession::default();
    let mut ws = session.load_workspace(repo.path()).await?;
    let main_head = commit_file(&mut ws, "main.txt").await?;
    ws.add_workspace("second".to_owned(), second_path.clone())
        .await?;

    assert!(second_path.join(".git").is_file());
    assert_eq!(git_worktree_count(repo.path())?, 1);
    assert!(second_path.join(".jj").join(".gitignore").exists());
    // the new working-copy commit is a sibling of ours
    assert_eq!(git_head(&ws, &second_path)?, Some(main_head.hex()));

    Ok(())
}

#[tokio::test]
async fn add_workspace_respects_colocate_setting() -> Result<()> {
    let repo = mkcolocated().await?;
    let (_second_dir, second_path) = mkdest("second")?;

    let mut session = WorkerSession::default();
    let mut ws = session.load_workspace(repo.path()).await?;
    let mut config = ws.data.workspace_settings.config().clone();
    config.add_layer(ConfigLayer::parse(
        ConfigSource::CommandArg,
        "git.colocate = false",
    )?);
    ws.data.workspace_settings = ws.data.workspace_settings.with_new_config(config)?;

    ws.add_workspace("second".to_owned(), second_path.clone())
        .await?;

    assert!(!second_path.join(".git").exists());
    assert_eq!(git_worktree_count(repo.path())?, 0);

    Ok(())
}

#[tokio::test]
async fn worktree_workspace_exports_its_own_head() -> Result<()> {
    let repo = mkcolocated().await?;
    let (_second_dir, second_path) = mkdest("second")?;

    let main_head = {
        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;
        let main_head = commit_file(&mut ws, "main.txt").await?;
        ws.add_workspace("second".to_owned(), second_path.clone())
            .await?;
        main_head
    };

    let mut session = WorkerSession::default();
    let mut ws = session.load_workspace(&second_path).await?;
    let second_head = commit_file(&mut ws, "second.txt").await?;

    assert_eq!(git_head(&ws, &second_path)?, Some(second_head.hex()));
    assert_eq!(git_head(&ws, repo.path())?, Some(main_head.hex()));

    Ok(())
}

#[tokio::test]
async fn forget_worktree_workspace_unlinks_worktree() -> Result<()> {
    let repo = mkcolocated().await?;
    let (_second_dir, second_path) = mkdest("second")?;

    let mut session = WorkerSession::default();
    let mut ws = session.load_workspace(repo.path()).await?;
    ws.add_workspace("second".to_owned(), second_path.clone())
        .await?;
    assert!(second_path.join(".git").is_file());

    ForgetWorkspace {
        name: "second".to_owned(),
    }
    .execute_unboxed(&mut ws)
    .await?;

    // git forgets the worktree, but its files are left in place
    assert!(!second_path.join(".git").exists());
    assert_eq!(git_worktree_count(repo.path())?, 0);
    assert!(second_path.join(".jj").exists());

    Ok(())
}
