use super::{mkrepo, revs};
use crate::messages::{RevHeader, RevResult, StoreRef};
use crate::worker::{queries, WorkerSession};
use anyhow::Result;
use assert_matches::assert_matches;

#[test]
fn log_all() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let all_rows = queries::query_log(&ws, "all()", 100)?;

    assert_eq!(12, all_rows.rows.len());
    assert!(!all_rows.has_more);

    Ok(())
}

#[test]
fn log_paged() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let page_rows = queries::query_log(&ws, "all()", 6)?;

    assert_eq!(6, page_rows.rows.len());
    assert!(page_rows.has_more);

    Ok(())
}

#[test]
fn log_subset() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let several_rows = queries::query_log(&ws, "bookmarks()", 100)?;

    assert_eq!(3, several_rows.rows.len());

    Ok(())
}

#[test]
fn log_mutable() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let single_row = queries::query_log(&ws, "mnkoropy", 100)?
        .rows
        .pop()
        .unwrap();

    assert!(!single_row.revision.is_immutable);

    Ok(())
}

#[test]
fn log_immutable() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let single_row = queries::query_log(&ws, "ummxkyyk", 100)?
        .rows
        .pop()
        .unwrap();

    assert!(single_row.revision.is_immutable);

    Ok(())
}

#[test]
fn revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::main_bookmark())?;

    assert_matches!(
        rev,
        RevResult::Detail {
            header: RevHeader { refs, .. },
            ..
        } if matches!(refs.as_slice(), [StoreRef::LocalBookmark { branch_name, .. }] if branch_name == "main")
    );

    Ok(())
}

#[test]
fn remotes_all() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let remotes = queries::query_remotes(&ws, None)?;

    assert_eq!(2, remotes.len());
    assert!(remotes.contains(&String::from("origin")));
    assert!(remotes.contains(&String::from("second")));

    Ok(())
}

#[test]
fn remotes_tracking_bookmark() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let remotes = queries::query_remotes(&ws, Some(String::from("main")))?;

    assert_eq!(1, remotes.len());
    assert!(remotes.contains(&String::from("origin")));

    Ok(())
}
