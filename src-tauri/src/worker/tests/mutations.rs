use super::{mkrepo, revs};
use crate::{
    messages::{
        AbandonRevisions, ChangeHunk, CheckoutRevision, CopyChanges, CreateRevision,
        DescribeRevision, DuplicateRevisions, FileRange, HunkLocation, InsertRevision, MoveChanges,
        MoveHunk, MoveSource, MultilineString, MutationResult, RevResult, TreePath,
    },
    worker::{Mutation, WorkerSession, queries},
};
use anyhow::{Result, anyhow};
use assert_matches::assert_matches;
use jj_lib::{backend::TreeValue, commit::Commit, repo::Repo, repo_path::RepoPath};
use std::fs;

// Helper function to read file content from a specific commit's tree
fn read_commit_file_content(
    repo: &dyn Repo,
    commit: &Commit,
    path: &str,
) -> Result<Option<String>> {
    let store = repo.store();
    let tree = commit.tree()?;
    let repo_path = RepoPath::from_internal_string(path)?;

    match tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(TreeValue::File { id, .. })) => {
            let mut reader = store.read_file(&repo_path, &id)?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content)?;
            Ok(Some(String::from_utf8(content)?))
        }
        Ok(Some(_)) => Ok(None), // Not a file (symlink, tree, git submodule, etc.)
        Ok(None) => Ok(None),    // Not found or not a file
        Err(conflict) => {
            // Handle conflict when reading file content
            Err(anyhow!(
                "Conflict reading file content for path: {}: {:?}",
                path,
                conflict
            ))
        }
    }
}

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
        path: TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk: ChangeHunk {
            location: HunkLocation {
                from_file: FileRange { start: 1, len: 1 },
                to_file: FileRange { start: 1, len: 6 },
            },
            lines: MultilineString {
                lines: vec![
                    "-<<<<<<< Conflict 1 of 1".to_owned(),
                    "-+++++++ Contents of side #1".to_owned(),
                    " 11".to_owned(),
                    "-%%%%%%% Changes from base to side #2".to_owned(),
                    "- 1".to_owned(),
                    "-2".to_owned(),
                    "->>>>>>> Conflict 1 of 1 ends".to_owned(),
                    "+2".to_owned(),
                ],
            },
        },
    };

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify that the file content of b.txt in the working directory now reflects the mutation
    let file_path = repo.path().join("b.txt");
    let content = std::fs::read_to_string(&file_path)?;
    assert!(
        !content.contains("old line"),
        "File should not contain 'old line'"
    );
    assert!(
        content.contains("new line"),
        "File should contain 'new line'"
    );

    Ok(())
}

#[test]
fn move_hunk_insertion_position() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Define the source and target commits (using known IDs from mkrepo)
    let from_commit_info = revs::main_bookmark(); // Commit that added c.txt and modified b.txt
    let to_commit_info = revs::working_copy(); // Child of main_bookmark

    // Define the path and the hunk to move
    // Hunk represents adding "2" and removing "1\n2" in b.txt (from main_bookmark commit)
    let file_path_str = "b.txt";
    let hunk = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 2 }, // Start line 1, length 2 in parent
            to_file: FileRange { start: 1, len: 1 },   // Start line 1, length 1 in child
        },
        lines: MultilineString {
            lines: vec!["-1".to_owned(), "-2".to_owned(), "+2".to_owned()],
        },
    };

    // Execute the mutation
    let mutation = MoveHunk {
        from_id: from_commit_info.clone(), // Use the original commit info
        to_id: to_commit_info.clone().commit,
        path: TreePath {
            repo_path: file_path_str.to_owned(),
            relative_path: "".into(),
        },
        hunk: hunk.clone(),
    };
    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // --- Verification ---
    // Get the potentially rewritten commit objects
    let rewritten_from_commit = ws.resolve_single_commit(&from_commit_info.commit)?;
    let rewritten_to_commit = ws.resolve_single_commit(&to_commit_info.commit)?;

    // Read content from the commit trees
    let from_content = read_commit_file_content(ws.repo(), &rewritten_from_commit, file_path_str)?
        .expect("b.txt should exist in source commit");
    let to_content = read_commit_file_content(ws.repo(), &rewritten_to_commit, file_path_str)?
        .expect("b.txt should exist in target commit");

    // Assertions
    assert!(
        !from_content.contains("\n2"),
        "Source content should not contain the moved hunk part ('2')"
    );
    assert!(
        from_content.contains("1"),
        "Source content should still contain the original line '1'"
    ); // Assumes original content was "1\n2"
    assert!(
        to_content.contains("\n2"),
        "Target content should contain the moved hunk part ('2')"
    );
    assert!(
        !to_content.contains("1\n"),
        "Target content should not contain the removed line '1' from the hunk"
    );

    // Check parent relationship if needed (especially for descendant case)
    // let to_parents = rewritten_to_commit.parents()?;
    // assert!(to_parents.iter().any(|p| p.id() == rewritten_from_commit.id()));

    Ok(())
}

#[test]
fn move_hunk_to_descendant() -> anyhow::Result<()> {
    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Scenario: A -> B. Move hunk from A to B.
    // A is main_bookmark (modified b.txt)
    // B is working_copy (child of A, no changes initially relative to A)
    let commit_a_info = revs::main_bookmark();
    let commit_b_info = revs::working_copy();
    let file_path_str = "b.txt";

    // Hunk representing the change in b.txt made by commit A
    let hunk_a = ChangeHunk {
        location: HunkLocation {
            from_file: FileRange { start: 1, len: 2 }, // In A's parent (root)
            to_file: FileRange { start: 1, len: 1 },   // In A
        },
        lines: MultilineString {
            lines: vec!["-1".to_owned(), "-2".to_owned(), "+2".to_owned()],
        },
    };

    // Execute the mutation: Move hunk from A to B
    let mutation = MoveHunk {
        from_id: commit_a_info.clone(),
        to_id: commit_b_info.clone().commit,
        path: TreePath {
            repo_path: file_path_str.to_owned(),
            relative_path: "".into(),
        },
        hunk: hunk_a.clone(),
    };
    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // --- Verification ---
    // Get the potentially rewritten commit objects
    let commit_a_prime = ws.resolve_single_commit(&commit_a_info.commit)?;
    let commit_b_prime = ws.resolve_single_commit(&commit_b_info.commit)?;

    // Read content from the commit trees
    let a_prime_content = read_commit_file_content(ws.repo(), &commit_a_prime, file_path_str)?
        .expect("b.txt should exist in source commit A'");
    let b_prime_content = read_commit_file_content(ws.repo(), &commit_b_prime, file_path_str)?
        .expect("b.txt should exist in target commit B'");

    // Assertions
    // A' (source) should now look like its parent (root commit) regarding b.txt
    assert!(
        !a_prime_content.contains("\n2"),
        "A' content should not contain the moved hunk part ('2')"
    );
    assert!(
        a_prime_content.contains("1"),
        "A' content should revert to the base state ('1')"
    );

    // B' (target) should retain the original content of B (which included the hunk from A)
    assert!(
        b_prime_content.contains("\n2"),
        "B' content should still contain the hunk part ('2')"
    );
    assert!(
        !b_prime_content.contains("1\n"),
        "B' content should still not contain the line '1' removed by the hunk"
    );

    // Verify parent relationship: B's parent should now be A'
    let parent_id = commit_b_prime
        .parents()
        .next()
        .ok_or_else(|| anyhow!("No parent found"))?
        .map_err(|e| anyhow!(e))?
        .id()
        .clone();
    assert_eq!(
        parent_id,
        commit_a_prime.id().to_owned(),
        "B\'s parent should be A\'"
    );

    Ok(())
}

// XXX missing tests for:
// - branch/ref mutations
// - git interop
