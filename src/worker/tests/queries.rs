use super::{mkrepo, revs};
use crate::messages::{RevHeader, RevResult, StoreRef};
use crate::worker::{WorkerSession, queries};
use anyhow::Result;
use assert_matches::assert_matches;

#[test]
fn log_all() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let all_rows = queries::query_log(&ws, "all()", 100)?;

    assert_eq!(19, all_rows.rows.len());
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

    assert_eq!(4, several_rows.rows.len());

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

#[tokio::test]
async fn revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::main_bookmark()).await?;

    assert_matches!(
        rev,
        RevResult::Detail {
            header: RevHeader { refs, .. },
            ..
        } if matches!(refs.as_slice(), [StoreRef::LocalBookmark { branch_name, .. }] if branch_name == "main")
    );

    Ok(())
}

/// Test that querying a conflicted revision includes conflict markers in the hunks.
/// The conflict labels from the trees should be passed through to materialize_tree_value().
///
/// Note: The test repo was created before jj stored conflict labels, so conflicts use
/// fallback labels like "side #1" and "base" instead of commit-specific labels.
/// A future improvement would be to recreate the test repo with labeled conflicts
/// and assert on the actual label content (e.g., change ID and commit ID).
#[tokio::test]
async fn revision_with_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::conflict_bookmark()).await?;

    let RevResult::Detail {
        header,
        changes: _,
        conflicts,
        ..
    } = rev
    else {
        panic!("Expected RevResult::Detail");
    };

    // The conflict_bookmark commit should be marked as having conflicts
    assert!(
        header.has_conflict,
        "Expected header.has_conflict to be true"
    );

    // The conflicts field should contain the conflict info (inherited from parent)
    assert!(!conflicts.is_empty(), "Expected conflicts to be non-empty");

    // The conflict hunks should contain conflict markers (<<<<<<< and >>>>>>>)
    let conflict_lines: String = conflicts
        .iter()
        .flat_map(|c| &c.hunk.lines.lines)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        conflict_lines.contains("<<<<<<<") && conflict_lines.contains(">>>>>>>"),
        "Expected conflict markers in conflict hunks, got: {conflict_lines}"
    );

    Ok(())
}

/// Test that resolving a conflict produces a change diff that includes conflict markers
/// from the "before" side (the conflicted parent tree).
///
/// Note: Same caveat as revision_with_conflict - the test repo lacks stored conflict labels,
/// so we only verify that conflict markers appear, not their specific label content.
#[tokio::test]
async fn revision_resolves_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    // resolve_conflict is a child of conflict_bookmark that resolves the conflict
    let rev = queries::query_revision(&ws, revs::resolve_conflict()).await?;

    let RevResult::Detail {
        header, changes, ..
    } = rev
    else {
        panic!("Expected RevResult::Detail");
    };

    // This commit resolved the conflict, so it should not be conflicted
    assert!(
        !header.has_conflict,
        "Expected header.has_conflict to be false for resolved commit"
    );

    // There should be at least one change (the conflict resolution)
    assert!(!changes.is_empty(), "Expected at least one change");

    // Find the change for the file that had the conflict
    // The diff shows going FROM conflicted parent TO resolved commit
    // So the "before" side (parent tree) had conflict markers
    let all_lines: String = changes
        .iter()
        .flat_map(|c| &c.hunks)
        .flat_map(|h| &h.lines.lines)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    // The diff should show removal of conflict markers (as deleted lines with -)
    // This verifies that format_tree_changes properly materialized the conflicted
    // "before" tree with its labels
    assert!(
        all_lines.contains("-<<<<<<<") || all_lines.contains("->>>>>>>"),
        "Expected removed conflict markers in diff (showing resolution), got:\n{all_lines}"
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
