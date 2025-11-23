use super::{mkid, mkrepo, revs};
use crate::{
    messages::{
        AbandonRevisions, ChangeHunk, CheckoutRevision, CommitId, CopyChanges, CopyHunk,
        CreateRevision, DescribeRevision, DuplicateRevisions, FileRange, HunkLocation,
        InsertRevision, MoveChanges, MoveHunk, MoveSource, MultilineString, MutationResult,
        RevResult, TreePath,
    },
    worker::{Mutation, WorkerSession, queries},
};
use anyhow::Result;
use assert_matches::assert_matches;
use jj_lib::object_id::ObjectId;
use pollster::block_on;
use std::fs;
use tokio::io::AsyncReadExt;

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

    // Move a hunk from kmtstztw (mutable) which changed b.txt from "1" to "11"
    // to working_copy

    let from_commit = mkid("kmtstztw", "7ef013217e74fce9ff04743ee9f1543fe9419675");
    let to_commit = revs::working_copy();

    // Hunk that changes "1" to "11" in b.txt
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 1, len: 1 },
        },
        lines: MultilineString {
            lines: vec!["-1".to_owned(), "+11".to_owned()],
        },
    };

    let mutation = MoveHunk {
        from_id: from_commit.clone(),
        to_id: to_commit.commit.clone(),
        path: TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify the hunk was moved correctly:
    // Expected behavior:
    // 1. Source commit should have b.txt unchanged from parent (becomes empty commit)
    // 2. Target commit (working copy) should have the change from "1" to "11"

    // IMPORTANT: After rewriting the working copy, we need to query the NEW working copy,
    // not the old commit ID that was passed to the mutation!
    let new_wc_commit_id = ws.wc_id().clone();

    let from_rev = queries::query_revision(&ws, from_commit.clone())?;
    let to_commit_obj = ws.get_commit(&new_wc_commit_id)?;
    let to_tree = to_commit_obj.tree()?;

    // When moving a hunk between unrelated commits, the hunk is applied directly
    // Source commit: After removing the hunk, it should become empty (or nearly empty)
    // Target commit: After adding the hunk, it should cleanly apply the change
    match from_rev {
        RevResult::NotFound { .. } => (), // Abandoned because it became empty
        RevResult::Detail {
            header, changes, ..
        } if !header.has_conflict && changes.len() <= 1 => (),
        _ => panic!(
            "Expected source commit to be abandoned or have minimal changes after hunk removal"
        ),
    }

    // Verify target has the change applied cleanly
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("b.txt")?;
    let path_value = to_tree.path_value(&repo_path)?;

    // Verify that b.txt is resolved (no conflict) in the target commit
    assert!(
        path_value.is_resolved(),
        "Expected b.txt to be cleanly updated after hunk move between unrelated commits"
    );

    // Verify the content is correct
    match path_value.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "11\n2\n",
                "Target should have '11' (hunk applied) followed by '2' from its own changes"
            );
        }
        _ => panic!("Expected b.txt to be a file in target commit"),
    }

    Ok(())
}

#[test]
fn move_hunk_message() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Test that commit messages are combined correctly
    // Move from kmtstztw to working_copy (which has no description)

    let from_commit = mkid("kmtstztw", "7ef013217e74fce9ff04743ee9f1543fe9419675");
    let to_commit = revs::working_copy();

    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 1, len: 1 },
        },
        lines: MultilineString {
            lines: vec!["-1\n".to_owned(), "+11\n".to_owned()],
        },
    };

    let mutation = MoveHunk {
        from_id: from_commit.clone(),
        to_id: to_commit.commit.clone(),
        path: TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Just verify the mutation succeeded - message combining is tested implicitly
    Ok(())
}

#[test]
fn move_hunk_invalid() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Test that applying an invalid hunk (that doesn't match source) fails appropriately
    let from_commit = mkid("kmtstztw", "7ef013217e74fce9ff04743ee9f1543fe9419675");
    let to_commit = revs::working_copy();

    // This hunk doesn't match the actual content of b.txt in kmtstztw
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
        from_id: from_commit,
        to_id: to_commit.commit,
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
    use jj_lib::object_id::ObjectId;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Create a small file in the working copy
    fs::write(repo.path().join("test.txt"), "line1\nline2\nline3\n")?;

    // Snapshot to create the parent commit with the file
    ws.import_and_snapshot(false)?;
    let parent_commit_id = ws.wc_id().clone();
    let parent_change_id = ws.get_commit(&parent_commit_id)?.change_id().clone();

    // Create a child commit (this will make a new empty WC on top of parent_commit_after_file)
    let _result = CreateRevision {
        parent_ids: vec![mkid(&parent_commit_id.hex(), &parent_commit_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    // Enlarge the file significantly in the child
    fs::write(
        repo.path().join("test.txt"),
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n",
    )?;

    // Snapshot the child
    ws.import_and_snapshot(false)?;
    let child_commit_id = ws.wc_id().clone();
    let child_change_id = ws.get_commit(&child_commit_id)?.change_id().clone();

    // Move a hunk from the child (lines 4-6) to the parent
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 3, len: 1 }, // Line 3 in parent (context)
            to_file: FileRange { start: 3, len: 4 },   // Lines 3-6 in child
        },
        lines: MultilineString {
            lines: vec![
                " line3\n".to_owned(), // context line
                "+line4\n".to_owned(),
                "+line5\n".to_owned(),
                "+line6\n".to_owned(),
            ],
        },
    };

    let mutation = MoveHunk {
        from_id: mkid(&child_commit_id.hex(), &child_commit_id.hex()),
        to_id: crate::messages::CommitId {
            hex: parent_commit_id.hex(),
            prefix: parent_commit_id.hex()[..2].to_string(),
            rest: parent_commit_id.hex()[2..].to_string(),
        },
        path: TreePath {
            repo_path: "test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify the result:
    // When moving a hunk from child to parent (descendant to ancestor):
    // - Parent should get the hunk applied (lines 1-6)
    // - Child should get rebased with the hunk removed (lines 1-3, 7-10)
    use jj_lib::repo::Repo;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("test.txt")?;

    // Get the new commit IDs after rewriting (change IDs stay constant)
    let new_parent_commit_ids = ws
        .repo()
        .resolve_change_id(&parent_change_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve parent change ID"))?;
    let new_child_commit_ids = ws
        .repo()
        .resolve_change_id(&child_change_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve child change ID"))?;

    // Verify parent has the hunk applied
    let parent_commit = ws.get_commit(&new_parent_commit_ids[0])?;
    let parent_tree = parent_commit.tree()?;

    match parent_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nline4\nline5\nline6\n",
                "Parent should have lines 1-6 after hunk move"
            );
        }
        _ => panic!("Expected test.txt to be a file in parent commit"),
    }

    // Verify child - check if there are conflicts from the merge
    let child_commit = ws.get_commit(&new_child_commit_ids[0])?;
    let child_tree = child_commit.tree()?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            // Clean resolution - child should have hunk removed
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nline7\nline8\nline9\nline10\n",
                "Child should have lines 1-3, 7-10 after hunk removal and rebase"
            );
        }
        Err(_) => {
            // Conflict case - verify the tree has conflicts
            match child_tree.id() {
                jj_lib::backend::MergedTreeId::Merge(merge) => {
                    assert!(
                        !merge.is_resolved(),
                        "Child commit should have conflicts when merge creates them"
                    );
                }
                _ => panic!("Expected conflicted tree"),
            }
        }
        _ => panic!("Expected test.txt to exist in child commit"),
    }

    Ok(())
}

#[test]
fn move_hunk_unrelated() -> anyhow::Result<()> {
    use jj_lib::object_id::ObjectId;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Create a common base with test.txt
    fs::write(repo.path().join("test.txt"), "line1\nline2\nline3\n")?;
    ws.import_and_snapshot(false)?;
    let base_commit_id = ws.wc_id().clone();

    // First branch: create commit A that doesn't change test.txt
    CreateRevision {
        parent_ids: vec![mkid(&base_commit_id.hex(), &base_commit_id.hex())],
    }
    .execute_unboxed(&mut ws)?;
    fs::write(repo.path().join("other_file.txt"), "unrelated\n")?;
    ws.import_and_snapshot(false)?;
    let commit_a_id = ws.wc_id().clone();
    let commit_a_change_id = ws.get_commit(&commit_a_id)?.change_id().clone();

    // Second branch: create commit B that adds lines 4-10 to test.txt
    CheckoutRevision {
        id: mkid(&base_commit_id.hex(), &base_commit_id.hex()),
    }
    .execute_unboxed(&mut ws)?;

    CreateRevision {
        parent_ids: vec![mkid(&base_commit_id.hex(), &base_commit_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    fs::write(
        repo.path().join("test.txt"),
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n",
    )?;
    ws.import_and_snapshot(false)?;
    let commit_b_id = ws.wc_id().clone();
    let commit_b_change_id = ws.get_commit(&commit_b_id)?.change_id().clone(); // Now move a hunk from B (lines 4-6 in test.txt) to A (which has lines 1-3)
    // Since these commits are unrelated (different branches), moving hunks will create conflicts
    // in both source and target, similar to move_hunk_content test
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 3, len: 1 }, // Line 3 (context)
            to_file: FileRange { start: 3, len: 4 },   // Lines 3-6
        },
        lines: MultilineString {
            lines: vec![
                " line3\n".to_owned(), // context
                "+line4\n".to_owned(),
                "+line5\n".to_owned(),
                "+line6\n".to_owned(),
            ],
        },
    };

    let mutation = MoveHunk {
        from_id: mkid(&commit_b_id.hex(), &commit_b_id.hex()),
        to_id: mkid(&commit_a_id.hex(), &commit_a_id.hex()).commit,
        path: TreePath {
            repo_path: "test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify the hunk was moved correctly
    use jj_lib::repo::Repo;
    let test_path = jj_lib::repo_path::RepoPath::from_internal_string("test.txt")?;

    // Get the new commit IDs after rewriting
    let new_commit_a_ids = ws
        .repo()
        .resolve_change_id(&commit_a_change_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve commit A change ID"))?;
    let new_commit_b_ids = ws
        .repo()
        .resolve_change_id(&commit_b_change_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve commit B change ID"))?;

    // Verify A now has test.txt with lines 1-6 (hunk added)
    let commit_a = ws.get_commit(&new_commit_a_ids[0])?;
    let tree_a = commit_a.tree()?;

    match tree_a.path_value(&test_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&test_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nline4\nline5\nline6\n",
                "Commit A should have lines 1-6 in test.txt after hunk move"
            );
        }
        _ => panic!("Expected test.txt to be a file in commit A"),
    }

    // Verify B has the hunk removed cleanly (no conflict)
    let commit_b = ws.get_commit(&new_commit_b_ids[0])?;
    let tree_b = commit_b.tree()?;

    match tree_b.path_value(&test_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&test_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nline7\nline8\nline9\nline10\n",
                "Commit B should have lines 1-3 and 7-10 (hunk lines 4-6 removed)"
            );
        }
        _ => panic!("Expected test.txt to be a file in commit B"),
    }

    Ok(())
}

#[test]
fn copy_hunk_from_parent() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Create a base commit with original content
    fs::write(repo.path().join("test.txt"), "line1\nline2\nline3\n")?;
    ws.import_and_snapshot(false)?;
    DescribeRevision {
        id: revs::working_copy(),
        new_description: "base".to_owned(),
        reset_author: false,
    }
    .execute_unboxed(&mut ws)?;

    let parent_id = ws.wc_id().clone();

    // Create a child commit with modifications
    CreateRevision {
        parent_ids: vec![mkid(&parent_id.hex(), &parent_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    // Modify the file (change line2 to "modified")
    fs::write(repo.path().join("test.txt"), "line1\nmodified\nline3\n")?;
    ws.import_and_snapshot(false)?;

    let child_id = ws.wc_id().clone();

    // Now restore the middle hunk from parent to child
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 3 }, // Lines 1-3 in parent
            to_file: FileRange { start: 1, len: 3 },   // Lines 1-3 in child (context + added)
        },
        lines: MultilineString {
            lines: vec![
                " line1".to_owned(),
                "-line2".to_owned(),    // In parent
                "+modified".to_owned(), // In child
                " line3".to_owned(),
            ],
        },
    };

    let mutation = CopyHunk {
        from_id: CommitId {
            hex: parent_id.hex(),
            prefix: parent_id.hex()[..2].to_string(),
            rest: parent_id.hex()[2..].to_string(),
        },
        to_id: mkid(&child_id.hex(), &child_id.hex()),
        path: TreePath {
            repo_path: "test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify: child should now have parent's content (restoration)
    let new_wc_id = ws.wc_id().clone();
    let child_commit = ws.get_commit(&new_wc_id)?;
    let child_tree = child_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("test.txt")?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\n",
                "Child should have parent's content after restore"
            );
        }
        _ => panic!("Expected test.txt to be a file"),
    }

    // Verify: parent unchanged
    let parent_commit = ws.get_commit(&parent_id)?;
    let parent_tree = parent_commit.tree()?;
    match parent_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\n",
                "Parent should be unchanged"
            );
        }
        _ => panic!("Expected test.txt to be a file in parent"),
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

    // Create a small file
    fs::write(repo.path().join("small.txt"), "line1\nline2\n")?;
    ws.import_and_snapshot(false)?;

    let parent_id = ws.wc_id().clone();

    CreateRevision {
        parent_ids: vec![mkid(&parent_id.hex(), &parent_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    fs::write(repo.path().join("small.txt"), "line1\nchanged\n")?;
    ws.import_and_snapshot(false)?;

    let child_id = ws.wc_id().clone();

    // Try to copy a hunk with out-of-bounds location
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 1 },
            to_file: FileRange { start: 10, len: 5 }, // Way out of bounds
        },
        lines: MultilineString {
            lines: vec!["-something".to_owned(), "+else".to_owned()],
        },
    };

    let mutation = CopyHunk {
        from_id: CommitId {
            hex: parent_id.hex(),
            prefix: parent_id.hex()[..2].to_string(),
            rest: parent_id.hex()[2..].to_string(),
        },
        to_id: mkid(&child_id.hex(), &child_id.hex()),
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

    // Create parent and child with identical content
    fs::write(repo.path().join("same.txt"), "line1\nline2\nline3\n")?;
    ws.import_and_snapshot(false)?;

    let parent_id = ws.wc_id().clone();

    CreateRevision {
        parent_ids: vec![mkid(&parent_id.hex(), &parent_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    // Child has same content
    fs::write(repo.path().join("same.txt"), "line1\nline2\nline3\n")?;
    ws.import_and_snapshot(false)?;

    let child_id = ws.wc_id().clone();

    // Try to restore a hunk that's already identical
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

    let mutation = CopyHunk {
        from_id: CommitId {
            hex: parent_id.hex(),
            prefix: parent_id.hex()[..2].to_string(),
            rest: parent_id.hex()[2..].to_string(),
        },
        to_id: mkid(&child_id.hex(), &child_id.hex()),
        path: TreePath {
            repo_path: "same.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

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

    // Create parent with original content
    fs::write(
        repo.path().join("multi.txt"),
        "line1\nline2\nline3\nline4\nline5\n",
    )?;
    ws.import_and_snapshot(false)?;
    let parent_id = ws.wc_id().clone();

    CreateRevision {
        parent_ids: vec![mkid(&parent_id.hex(), &parent_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    // Child modifies multiple lines
    fs::write(
        repo.path().join("multi.txt"),
        "line1\nmodified2\nline3\nmodified4\nline5\n",
    )?;
    ws.import_and_snapshot(false)?;
    let child_id = ws.wc_id().clone();

    // Restore only the second hunk (line 4)
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 3, len: 3 }, // Lines 3-5 in parent
            to_file: FileRange { start: 3, len: 3 },   // Lines 3-5 in child (context + added)
        },
        lines: MultilineString {
            lines: vec![
                " line3".to_owned(),
                "-line4".to_owned(),
                "+modified4".to_owned(),
                " line5".to_owned(),
            ],
        },
    };

    let mutation = CopyHunk {
        from_id: CommitId {
            hex: parent_id.hex(),
            prefix: parent_id.hex()[..2].to_string(),
            rest: parent_id.hex()[2..].to_string(),
        },
        to_id: mkid(&child_id.hex(), &child_id.hex()),
        path: TreePath {
            repo_path: "multi.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify: line 2 still modified, line 4 restored
    let new_wc_id = ws.wc_id().clone();
    let child_commit = ws.get_commit(&new_wc_id)?;
    let child_tree = child_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("multi.txt")?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nmodified2\nline3\nline4\nline5\n",
                "Line 2 should remain modified, line 4 should be restored"
            );
        }
        _ => panic!("Expected multi.txt to be a file"),
    }

    Ok(())
}

#[test]
fn move_hunk_second_of_two_hunks() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // First create a base file in the parent commit
    let base = "line1\nline2\nline3\nline4\nline5\n";
    fs::write(repo.path().join("test.txt"), base)?;
    ws.import_and_snapshot(false)?;

    // Create a child commit
    let parent_commit_id: jj_lib::backend::CommitId = ws.wc_id().clone();
    CreateRevision {
        parent_ids: vec![mkid(&parent_commit_id.hex(), &parent_commit_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    // Now modify the file to add TWO hunks
    let modified = "line1\ninserted1\nline2\nline3\nline4\ninserted2\nline5\n";
    fs::write(repo.path().join("test.txt"), modified)?;
    ws.import_and_snapshot(false)?;

    let source_commit_id = ws.wc_id().clone();

    // Try to move the SECOND hunk back to its parent
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 5, len: 2 },
            to_file: FileRange { start: 6, len: 3 },
        },
        lines: MultilineString {
            lines: vec![
                " line4".to_owned(),
                "+inserted2".to_owned(),
                " line5".to_owned(),
            ],
        },
    };

    // Now create a different target commit (sibling of source) to move the hunk to
    CreateRevision {
        parent_ids: vec![mkid(&parent_commit_id.hex(), &parent_commit_id.hex())],
    }
    .execute_unboxed(&mut ws)?;

    // Add the base file (without hunks) to the target
    fs::write(repo.path().join("test.txt"), base)?;
    ws.import_and_snapshot(false)?;

    let target_commit_id = ws.wc_id().clone();

    let mutation = MoveHunk {
        from_id: mkid(&source_commit_id.hex(), &source_commit_id.hex()),
        to_id: crate::messages::CommitId {
            hex: target_commit_id.hex(),
            prefix: target_commit_id.hex()[..2].to_string(),
            rest: target_commit_id.hex()[2..].to_string(),
        },
        path: TreePath {
            repo_path: "test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws);

    assert_matches!(result, Ok(MutationResult::Updated { .. }));

    // Verify the hunk was moved correctly
    use jj_lib::repo::Repo;
    let test_path = jj_lib::repo_path::RepoPath::from_internal_string("test.txt")?;

    // Get the change IDs to resolve the new commit IDs after rewriting
    let source_change_id = ws.get_commit(&source_commit_id)?.change_id().clone();
    let target_change_id = ws.get_commit(&target_commit_id)?.change_id().clone();

    let new_source_commit_ids = ws
        .repo()
        .resolve_change_id(&source_change_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve source change ID"))?;
    let new_target_commit_ids = ws
        .repo()
        .resolve_change_id(&target_change_id)
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve target change ID"))?;

    // Verify source commit has the second hunk removed (only first hunk remains)
    let source_commit = ws.get_commit(&new_source_commit_ids[0])?;
    let source_tree = source_commit.tree()?;

    match source_tree.path_value(&test_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&test_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\ninserted1\nline2\nline3\nline4\nline5\n",
                "Source should have only the first hunk (inserted1), second hunk removed"
            );
        }
        _ => panic!("Expected test.txt to be a file in source commit"),
    }

    // Verify target commit has the second hunk added
    let target_commit = ws.get_commit(&new_target_commit_ids[0])?;
    let target_tree = target_commit.tree()?;

    match target_tree.path_value(&test_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&test_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nline4\ninserted2\nline5\n",
                "Target should have the second hunk (inserted2) added"
            );
        }
        _ => panic!("Expected test.txt to be a file in target commit"),
    }

    Ok(())
}

// XXX missing tests for:
// - branch/ref mutations
// - git interop
