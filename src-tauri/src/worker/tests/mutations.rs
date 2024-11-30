use super::{mkrepo, revs};
use crate::{
    messages::{
        AbandonRevisions, CheckoutRevision, CopyChanges, CreateRevision, DescribeRevision,
        DuplicateRevisions, InsertRevision, MoveChanges, MoveSource, MutationResult, RevResult,
        TreePath, MoveHunk, ChangeHunk, HunkLocation, FileRange, MultilineString,
    },
    worker::{queries, Mutation, WorkerSession},
};
use anyhow::Result;
use assert_matches::assert_matches;
use std::fs;

#[test]
fn abandon_revisions() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let page = queries::query_log(&ws, "all()", 100)?;
    assert_eq!(12, page.rows.len());

    AbandonRevisions {
        ids: vec![revs::resolve_conflict().commit],
    }
    .execute_unboxed(&mut ws)?;

    let page = queries::query_log(&ws, "all()", 100)?;
    assert_eq!(11, page.rows.len());

    Ok(())
}

#[test]
fn checkout_revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let head_rev = queries::query_revision(&ws, revs::working_copy())?;
    let conflict_rev = queries::query_revision(&ws, revs::conflict_bookmark())?;
    assert_matches!(head_rev, RevResult::Detail { header, .. } if header.is_working_copy);
    assert_matches!(conflict_rev, RevResult::Detail { header, .. } if !header.is_working_copy);

    let result = CheckoutRevision {
        id: revs::conflict_bookmark(),
    }
    .execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::UpdatedSelection { .. });

    let head_rev = queries::query_revision(&ws, revs::working_copy())?;
    let conflict_rev = queries::query_revision(&ws, revs::conflict_bookmark())?;
    assert_matches!(head_rev, RevResult::NotFound { .. });
    assert_matches!(conflict_rev, RevResult::Detail { header, .. } if header.is_working_copy);

    Ok(())
}

#[test]
fn copy_changes() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let from_rev = queries::query_revision(&ws, revs::resolve_conflict())?;
    let to_rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(from_rev, RevResult::Detail { changes, .. } if changes.len() == 1);
    assert_matches!(to_rev, RevResult::Detail { changes, .. } if changes.len() == 0);

    let result = CopyChanges {
        from_id: revs::resolve_conflict().commit,
        to_id: revs::working_copy(),
        paths: vec![TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        }],
    }
    .execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    let from_rev = queries::query_revision(&ws, revs::resolve_conflict())?;
    let to_rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(from_rev, RevResult::Detail { changes, .. } if changes.len() == 1);
    assert_matches!(to_rev, RevResult::Detail { changes, .. } if changes.len() == 1);

    Ok(())
}

#[test]
fn create_revision_single_parent() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let parent_rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(parent_rev, RevResult::Detail { header, .. } if header.is_working_copy);

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
fn create_revision_multi_parent() -> Result<()> {
    let repo: tempfile::TempDir = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let parent_rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(parent_rev, RevResult::Detail { header, .. } if header.is_working_copy);

    let result = CreateRevision {
        parent_ids: vec![revs::working_copy(), revs::conflict_bookmark()],
    }
    .execute_unboxed(&mut ws)?;

    match result {
        MutationResult::UpdatedSelection { new_selection, .. } => {
            let child_rev = queries::query_revision(&ws, new_selection.id)?;
            assert_matches!(child_rev, RevResult::Detail { parents, .. } if parents.len() == 2);
        }
        _ => assert!(false, "CreateRevision failed"),
    }

    Ok(())
}

#[test]
fn describe_revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0] == "");

    let result = DescribeRevision {
        id: revs::working_copy(),
        new_description: "wip".to_owned(),
        reset_author: false,
    }
    .execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    let rev = queries::query_revision(&ws, revs::working_copy())?;
    assert!(
        matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0] == "wip")
    );

    Ok(())
}

#[test]
fn describe_revision_with_snapshot() -> Result<()> {
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
fn duplicate_revisions() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0] == "");

    let result = DuplicateRevisions {
        ids: vec![revs::main_bookmark()],
    }
    .execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::UpdatedSelection { .. });

    let page = queries::query_log(&ws, "description(unsynced)", 3)?;
    assert_eq!(2, page.rows.len());

    Ok(())
}

#[test]
fn insert_revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let page = queries::query_log(&ws, "main::@", 4)?;
    assert_eq!(2, page.rows.len());

    InsertRevision {
        after_id: revs::main_bookmark(),
        before_id: revs::working_copy(),
        id: revs::resolve_conflict(),
    }
    .execute_unboxed(&mut ws)?;

    let page = queries::query_log(&ws, "main::@", 4)?;
    assert_eq!(3, page.rows.len());

    Ok(())
}

#[test]
fn move_changes_all_paths() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let parent_rev = queries::query_revision(&ws, revs::conflict_bookmark())?;
    assert_matches!(parent_rev, RevResult::Detail { header, .. } if header.has_conflict);

    let result = MoveChanges {
        from_id: revs::resolve_conflict(),
        to_id: revs::conflict_bookmark().commit,
        paths: vec![],
    }
    .execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    let parent_rev = queries::query_revision(&ws, revs::conflict_bookmark())?;
    assert_matches!(parent_rev, RevResult::Detail { header, .. } if !header.has_conflict);

    Ok(())
}

#[test]
fn move_changes_single_path() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let from_rev = queries::query_revision(&ws, revs::main_bookmark())?;
    let to_rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(from_rev, RevResult::Detail { changes, .. } if changes.len() == 2);
    assert_matches!(to_rev, RevResult::Detail { changes, .. } if changes.len() == 0);

    let result = MoveChanges {
        from_id: revs::main_bookmark(),
        to_id: revs::working_copy().commit,
        paths: vec![TreePath {
            repo_path: "c.txt".to_owned(),
            relative_path: "".into(),
        }],
    }
    .execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    let from_rev = queries::query_revision(&ws, revs::main_bookmark())?;
    let to_rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(from_rev, RevResult::Detail { changes, .. } if changes.len() == 1);
    assert_matches!(to_rev, RevResult::Detail { changes, .. } if changes.len() == 1);

    Ok(())
}

#[test]
fn move_source() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let page = queries::query_log(&ws, "@+", 1)?;
    assert_eq!(0, page.rows.len());

    MoveSource {
        id: revs::resolve_conflict(),
        parent_ids: vec![revs::working_copy().commit],
    }
    .execute_unboxed(&mut ws)?;

    let page = queries::query_log(&ws, "@+", 2)?;
    assert_eq!(1, page.rows.len());

    Ok(())
}

#[test]
fn move_hunk_single_line() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Prepare hunk mutation: remove "-old line" and add "+new line" in file b.txt
    let mutation = MoveHunk {
        from_id: revs::resolve_conflict(),
        to_id: revs::conflict_bookmark().commit,
        path: TreePath { repo_path: "b.txt".to_owned(), relative_path: "".into() },
        hunk: ChangeHunk {
            location: HunkLocation {
                from_file: FileRange { start: 1, len: 1 },
                to_file: FileRange { start: 1, len: 6 },
            },
            lines: MultilineString { lines: vec!["-<<<<<<< Conflict 1 of 1".to_owned(),
              "-+++++++ Contents of side #1".to_owned(),
              " 11".to_owned(),
              "-%%%%%%% Changes from base to side #2".to_owned(),
              "- 1".to_owned(),
              "-2".to_owned(),
              "->>>>>>> Conflict 1 of 1 ends".to_owned(),
              "+2".to_owned(),
            ] },
        },
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify that the file content of b.txt in the working directory now reflects the mutation
    let file_path = repo.path().join("b.txt");
    let content = std::fs::read_to_string(&file_path)?;
    assert!(!content.contains("old line"), "File should not contain 'old line'");
    assert!(content.contains("new line"), "File should contain 'new line'");

    Ok(())
}

#[test]
fn move_hunk_insertion_position() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Create known content for b.txt with three lines.
    let file_path = repo.path().join("b.txt");
    std::fs::write(&file_path, "first\nold line\nthird\n")?;

    // Construct a hunk to replace "old line" with "new line" at line 2.
    let mutation = MoveHunk {
        from_id: revs::resolve_conflict(),
        to_id: revs::conflict_bookmark().commit,
        path: TreePath { repo_path: "b.txt".to_owned(), relative_path: "".into() },
        hunk: ChangeHunk {
            location: HunkLocation {
                from_file: FileRange { start: 2, len: 1 },
                to_file: FileRange { start: 2, len: 1 },
            },
            lines: MultilineString { lines: vec![
                "-old line".to_owned(),
                "+new line".to_owned(),
            ] },
        },
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    let content = std::fs::read_to_string(&file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3, "Expected exactly three lines in b.txt");
    assert_eq!(lines[0], "first", "The first line should remain unchanged");
    assert_eq!(lines[1], "new line", "The inserted line should be at the correct position");
    assert_eq!(lines[2], "third", "The third line should remain unchanged");

    Ok(())
}

// XXX missing tests for:
// - branch/ref mutations
// - git interop
