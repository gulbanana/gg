use super::{get_rev, mkrepo, revs};
use crate::{
    messages::{
        AbandonRevisions, ChangeHunk, CheckoutRevision, CopyChanges, CopyHunk, CreateRevision,
        DescribeRevision, DuplicateRevisions, FileRange, HunkLocation, InsertRevision, MoveChanges,
        MoveHunk, MoveSource, MultilineString, MutationResult, RevResult, TreePath,
    },
    worker::{Mutation, WorkerSession, queries},
};
use anyhow::Result;
use assert_matches::assert_matches;
use pollster::block_on;
use std::fs;
use tokio::io::AsyncReadExt;

#[test]
fn abandon_revisions() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let page = queries::query_log(&ws, "all()", 100)?;
    assert_eq!(18, page.rows.len());

    AbandonRevisions {
        ids: vec![revs::resolve_conflict().commit],
    }
    .execute_unboxed(&mut ws)?;

    let page = queries::query_log(&ws, "all()", 100)?;
    assert_eq!(17, page.rows.len());

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
    assert_matches!(to_rev, RevResult::Detail { changes, .. } if changes.is_empty());

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
        _ => panic!("CreateRevision failed"),
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
        _ => panic!("CreateRevision failed"),
    }

    Ok(())
}

#[test]
fn describe_revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0].is_empty());

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
        matches!(rev, RevResult::Detail { header, changes, .. } if header.description.lines[0].is_empty() && changes.is_empty())
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
        matches!(rev, RevResult::Detail { header, changes, .. } if header.description.lines[0] == "wip" && !changes.is_empty())
    );

    Ok(())
}

#[test]
fn duplicate_revisions() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let rev = queries::query_revision(&ws, revs::working_copy())?;
    assert_matches!(rev, RevResult::Detail { header, .. } if header.description.lines[0].is_empty());

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
    assert_matches!(to_rev, RevResult::Detail { changes, .. } if changes.is_empty());

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
fn move_hunk_content() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_child_multi modifies lines 2 and 4: line2 -> changed2, line4 -> changed4
    // Move only the line 2 change to hunk_base (ancestor), keeping line 4 change in source
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 3 },
            to_file: FileRange { start: 1, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line1".to_owned(),
                "-line2".to_owned(),
                "+changed2".to_owned(),
                " line3".to_owned(),
            ],
        },
    };

    let mutation = MoveHunk {
        from_id: revs::hunk_child_multi(),
        to_id: revs::hunk_base().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source still has the line 4 change but not line 2
    let source_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let source_tree = source_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match source_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            // Source should have changed2 (inherited from rebased parent) and changed4 (its own change)
            assert_eq!(
                content_str, "line1\nchanged2\nline3\nchanged4\nline5\n",
                "Source should have both changes after rebase (parent now has changed2)"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file in source commit"),
    }

    // Verify target (hunk_base) has the line 2 change applied
    let target_commit = get_rev(&ws, &revs::hunk_base())?;
    let target_tree = target_commit.tree()?;

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nchanged2\nline3\nline4\nline5\n",
                "Target should have line 2 changed but not line 4"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file in target commit"),
    }

    Ok(())
}

#[test]
fn move_hunk_message() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Move only hunk from source, abandoning it
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 2, len: 1 },
            to_file: FileRange { start: 2, len: 1 },
        },
        lines: MultilineString {
            lines: vec!["-line2".to_owned(), "+modified2".to_owned()],
        },
    };

    let mutation = MoveHunk {
        from_id: revs::hunk_child_single(),
        to_id: revs::hunk_sibling().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Source should be abandoned (not found in repo)
    let source_rev = queries::query_revision(&ws, revs::hunk_child_single())?;
    assert_matches!(
        source_rev,
        RevResult::NotFound { .. },
        "Source should be abandoned"
    );

    // Target should have combined description
    let target_rev = queries::query_revision(&ws, revs::hunk_sibling())?;
    match target_rev {
        RevResult::Detail { header, .. } => {
            let desc = header.description.lines.join("\n");
            assert!(
                desc.contains("hunk sibling") && desc.contains("hunk child single"),
                "Target description should combine both: got '{}'",
                desc
            );
        }
        _ => panic!("Expected target to exist"),
    }

    Ok(())
}

#[test]
fn move_hunk_invalid() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Invalid hunk - doesn't match the actual content of b.txt in hunk_source
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 1, len: 1 },
        },
        lines: MultilineString {
            lines: vec!["-nonexistent".to_owned(), "+something".to_owned()],
        },
    };

    let mutation = MoveHunk {
        from_id: revs::hunk_source(),
        to_id: revs::working_copy().commit,
        path: TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws);
    assert!(result.is_err(), "Should fail with invalid hunk");

    Ok(())
}

#[test]
fn move_hunk_descendant() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_child_single's only change is line 2: "line2" -> "modified2"
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 3 },
            to_file: FileRange { start: 1, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line1".to_owned(),
                "-line2".to_owned(),
                "+modified2".to_owned(),
                " line3".to_owned(),
            ],
        },
    };

    // This should fail because it would leave the child empty
    let mutation = MoveHunk {
        from_id: revs::hunk_child_single(),
        to_id: revs::hunk_base().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::PreconditionError { .. });

    Ok(())
}

#[test]
fn move_hunk_unrelated() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_child_single modifies line 2: "line2" -> "modified2"
    // hunk_sibling extends the file with new6, new7, new8
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 3 },
            to_file: FileRange { start: 1, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line1".to_owned(),
                "-line2".to_owned(),
                "+modified2".to_owned(),
                " line3".to_owned(),
            ],
        },
    };

    // Move a hunk from hunk_child_single to hunk_sibling (unrelated commits, both children of hunk_base)
    let mutation = MoveHunk {
        from_id: revs::hunk_child_single(),
        to_id: revs::hunk_sibling().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source has the hunk removed (becomes empty and should be abandoned or have no changes)
    let from_rev = queries::query_revision(&ws, revs::hunk_child_single())?;
    match from_rev {
        RevResult::NotFound { .. } => (),
        RevResult::Detail { changes, .. } if changes.is_empty() => (),
        _ => panic!("Expected source commit to have no changes after hunk move"),
    }

    // Verify target has the hunk applied (with the new lines still there)
    let sibling_commit = get_rev(&ws, &revs::hunk_sibling())?;
    let sibling_tree = sibling_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match sibling_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nmodified2\nline3\nline4\nline5\nnew6\nnew7\nnew8\n",
                "Sibling should have modified2 plus the new lines"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file in sibling commit"),
    }

    Ok(())
}

#[test]
fn move_hunk_unrelated_different_structure_creates_conflict() -> anyhow::Result<()> {
    // This test documents that moving a hunk between unrelated commits with different
    // file structures will create a conflict. This is the correct semantic behavior
    // of 3-way merge: the hunk was computed against a different file than the target.
    //
    // Example: hunk_source's parent has b.txt="1\n", but working_copy has b.txt="1\n2\n"
    // Moving the "1->11" hunk creates a conflict because the target file has content
    // (line 2) that wasn't present when the hunk was computed.

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Hunk from hunk_source that changes line 1: "1" -> "11"
    // hunk_source's parent has b.txt = "1\n" (single line)
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 1, len: 1 },
        },
        lines: MultilineString {
            lines: vec!["-1".to_owned(), "+11".to_owned()],
        },
    };

    // Move to working_copy which has b.txt = "1\n2\n" (different structure)
    let mutation = MoveHunk {
        from_id: revs::hunk_source(),
        to_id: revs::working_copy().commit,
        path: TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // The target should have a conflict because the file structures differ
    let to_rev = queries::query_revision(&ws, revs::working_copy())?;
    match to_rev {
        RevResult::Detail { header, .. } => {
            assert!(
                header.has_conflict,
                "Expected conflict when moving hunk to file with different structure"
            );
        }
        _ => panic!("Expected working copy to exist"),
    }

    Ok(())
}

#[test]
fn copy_hunk_from_parent() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Copy/restore hunk from hunk_base (parent) to hunk_child_single (child)
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 3 },
            to_file: FileRange { start: 1, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line1".to_owned(),
                "-line2".to_owned(),
                "+modified2".to_owned(),
                " line3".to_owned(),
            ],
        },
    };

    // This should restore "line2" back (undo the "modified2" change)
    let mutation = CopyHunk {
        from_id: revs::hunk_base().commit,
        to_id: revs::hunk_child_single(),
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify: child should now have parent's content (restoration)
    let child_commit = get_rev(&ws, &revs::hunk_child_single())?;
    let child_tree = child_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nline4\nline5\n",
                "Child should have parent's content after restore"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file"),
    }

    Ok(())
}

#[test]
fn copy_hunk_to_conflict() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Use the existing conflict_bookmark from test repo
    let conflict_commit = revs::conflict_bookmark();
    let parent_commit = revs::main_bookmark();

    // Try to copy a hunk to the conflicted commit
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 1, len: 1 },
        },
        lines: MultilineString {
            lines: vec!["-original".to_owned(), "+changed".to_owned()],
        },
    };

    let mutation = CopyHunk {
        from_id: parent_commit.commit.clone(),
        to_id: conflict_commit.clone(),
        path: TreePath {
            repo_path: "b.txt".to_owned(), // This file has conflicts
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws);

    // Should fail with precondition error about conflicts
    match result {
        Ok(MutationResult::PreconditionError { message }) => {
            assert!(
                message.contains("conflict"),
                "Expected error message about conflicts, got: {}",
                message
            );
        }
        Ok(_) => panic!("Expected precondition error for conflicted file"),
        Err(e) => panic!("Expected precondition error, got hard error: {}", e),
    }

    Ok(())
}

#[test]
fn copy_hunk_out_of_bounds() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // small_parent has small.txt with "line1\nline2\n"
    // small_child has small.txt with "line1\nchanged\n"
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 10, len: 5 }, // Way out of bounds
        },
        lines: MultilineString {
            lines: vec!["-something".to_owned(), "+else".to_owned()],
        },
    };

    // Try to copy a hunk with out-of-bounds location using the small file commits
    let mutation = CopyHunk {
        from_id: revs::small_parent().commit,
        to_id: revs::small_child(),
        path: TreePath {
            repo_path: "small.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws);

    match result {
        Ok(MutationResult::PreconditionError { message }) => {
            assert!(
                message.contains("out of bounds"),
                "Expected error about bounds, got: {}",
                message
            );
        }
        Ok(_) => panic!("Expected precondition error for out of bounds"),
        Err(e) => panic!("Expected precondition error, got hard error: {}", e),
    }

    Ok(())
}

#[test]
fn copy_hunk_unchanged() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_base has line1-line5, hunk_sibling has line1-line5 plus new6-new8
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 3 },
            to_file: FileRange { start: 1, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line1".to_owned(),
                " line2".to_owned(),
                " line3".to_owned(),
            ],
        },
    };

    // Copy a hunk between hunk_base and hunk_sibling where that part is identical
    let mutation = CopyHunk {
        from_id: revs::hunk_base().commit,
        to_id: revs::hunk_sibling(),
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    // Trying to "restore" lines 1-3 from base to sibling should be unchanged (they're already identical)
    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Unchanged);

    Ok(())
}

#[test]
fn copy_hunk_multiple_hunks() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_child_multi modifies two lines: line2->changed2 and line4->changed4
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 3, len: 3 },
            to_file: FileRange { start: 3, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line3".to_owned(),
                "-line4".to_owned(),
                "+changed4".to_owned(),
                " line5".to_owned(),
            ],
        },
    };

    // Restore only the second hunk (line 4) from hunk_base
    let mutation = CopyHunk {
        from_id: revs::hunk_base().commit,
        to_id: revs::hunk_child_multi(),
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify: line 2 still modified (changed2), line 4 restored (line4)
    let child_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let child_tree = child_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nchanged2\nline3\nline4\nline5\n",
                "Line 2 should remain modified (changed2), line 4 should be restored"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file"),
    }

    Ok(())
}

#[test]
fn move_hunk_second_of_two_hunks() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_child_multi has two hunks: line2->changed2 and line4->changed4
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 3, len: 3 },
            to_file: FileRange { start: 3, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line3".to_owned(),
                "-line4".to_owned(),
                "+changed4".to_owned(),
                " line5".to_owned(),
            ],
        },
    };

    // Move only the second hunk (line4->changed4) to hunk_sibling
    let mutation = MoveHunk {
        from_id: revs::hunk_child_multi(),
        to_id: revs::hunk_sibling().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source still has the first hunk (changed2), but not the second
    let source_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let source_tree = source_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match source_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nchanged2\nline3\nline4\nline5\n",
                "Source should have first hunk (changed2) but not second"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file in source commit"),
    }

    // Verify target has the second hunk added
    let target_commit = get_rev(&ws, &revs::hunk_sibling())?;
    let target_tree = target_commit.tree()?;

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nchanged4\nline5\nnew6\nnew7\nnew8\n",
                "Target should have the second hunk (changed4) added plus new lines"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file in target commit"),
    }

    Ok(())
}

// XXX missing tests for:
// - branch/ref mutations
// - git interop
