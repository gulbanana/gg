use super::{get_rev, mkrepo, revs};
use crate::{
    messages::{
        AbandonRevisions, ChangeHunk, CheckoutRevision, CopyChanges, CopyHunk, CreateRevision,
        DescribeRevision, DuplicateRevisions, FileRange, HunkLocation, InsertRevision, MoveChanges,
        MoveHunk, MoveRef, MoveSource, MultilineString, MutationResult, RevId, RevSet, RevsResult,
        StoreRef, TreePath,
    },
    worker::{Mutation, WorkerSession, queries},
};
use anyhow::Result;
use assert_matches::assert_matches;
use jj_lib::str_util::StringMatcher;
use std::fs;
use tokio::io::AsyncReadExt;

/// Helper to get a single revision's display details (changes, conflicts, etc.)
async fn query_revision_details(
    ws: &crate::worker::gui_util::WorkspaceSession<'_>,
    id: RevId,
) -> Result<RevsResult> {
    queries::query_revisions(
        ws,
        RevSet {
            from: id.clone(),
            to: id,
        },
    )
    .await
}

#[tokio::test]
async fn abandon_revisions() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let page = queries::query_log(&ws, "all()", 100)?;
    assert_eq!(19, page.rows.len());

    AbandonRevisions {
        ids: vec![revs::resolve_conflict().commit],
    }
    .execute_unboxed(&mut ws)
    .await?;

    let page = queries::query_log(&ws, "all()", 100)?;
    assert_eq!(18, page.rows.len());

    Ok(())
}

#[tokio::test]
async fn checkout_revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let head_header = queries::query_revision(&ws, &revs::working_copy())?;
    let conflict_header = queries::query_revision(&ws, &revs::conflict_bookmark())?;
    assert!(head_header.expect("exists").is_working_copy);
    assert!(!conflict_header.expect("exists").is_working_copy);

    let result = CheckoutRevision {
        id: revs::conflict_bookmark(),
    }
    .execute_unboxed(&mut ws)
    .await?;
    assert_matches!(result, MutationResult::Updated { .. });

    let head_header = queries::query_revision(&ws, &revs::working_copy())?;
    let conflict_header = queries::query_revision(&ws, &revs::conflict_bookmark())?;
    assert!(
        head_header.is_none(),
        "old working copy revision should not exist"
    );
    assert!(conflict_header.expect("exists").is_working_copy);

    Ok(())
}

#[tokio::test]
async fn copy_changes() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let from_rev = query_revision_details(&ws, revs::resolve_conflict()).await?;
    let to_rev = query_revision_details(&ws, revs::working_copy()).await?;
    assert_matches!(from_rev, RevsResult::Detail { changes, .. } if changes.len() == 1);
    assert_matches!(to_rev, RevsResult::Detail { changes, .. } if changes.is_empty());

    let result = CopyChanges {
        from_id: revs::resolve_conflict().commit,
        to_id: revs::working_copy(),
        paths: vec![TreePath {
            repo_path: "b.txt".to_owned(),
            relative_path: "".into(),
        }],
    }
    .execute_unboxed(&mut ws)
    .await?;
    assert_matches!(result, MutationResult::Updated { .. });

    let from_rev = query_revision_details(&ws, revs::resolve_conflict()).await?;
    let to_rev = query_revision_details(&ws, revs::working_copy()).await?;
    assert_matches!(from_rev, RevsResult::Detail { changes, .. } if changes.len() == 1);
    assert_matches!(to_rev, RevsResult::Detail { changes, .. } if changes.len() == 1);

    Ok(())
}

#[tokio::test]
async fn immutability_of_bookmark() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let immutable_matcher = StringMatcher::Exact("immutable_bookmark".to_string());
    let (_, ref_at_start) = ws
        .view()
        .local_bookmarks_matching(&immutable_matcher)
        .next()
        .unwrap();
    let ref_at_start = ref_at_start.as_normal().unwrap().clone();
    assert_matches!(ws.check_immutable([ref_at_start.clone()]), Ok(true));

    let header = queries::query_revision(&ws, &revs::immutable_bookmark())?
        .expect("immutable_bookmark exists");
    let immutable_bm = header
        .refs
        .iter()
        .find(|r| {
            matches!(
                r,
                StoreRef::LocalBookmark {
                    branch_name,
                    ..
                    } if branch_name == "immutable_bookmark"
            )
        })
        .unwrap();

    let MutationResult::Updated {
        new_selection: Some(new_selection),
        ..
    } = CreateRevision {
        parent_ids: vec![revs::working_copy()],
    }
    .execute_unboxed(&mut ws)
    .await?
    else {
        panic!("Creating new revision didn't update the selection");
    };

    MoveRef {
        r#ref: immutable_bm.clone(),
        to_id: new_selection.id.clone(),
    }
    .execute_unboxed(&mut ws)
    .await?;

    let (_, after_change) = ws
        .view()
        .local_bookmarks_matching(&immutable_matcher)
        .next()
        .unwrap();
    let after_change = after_change.as_normal().unwrap().clone();
    assert_ne!(ref_at_start, after_change);

    assert_matches!(ws.check_immutable([after_change]), Ok(true));

    Ok(())
}

#[tokio::test]
async fn immutable_workspace_head() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let immutable_matcher = StringMatcher::Exact("immutable_bookmark".to_string());

    let header = queries::query_revision(&ws, &revs::immutable_bookmark())?
        .expect("immutable_bookmark exists");
    let immutable_bm = header
        .refs
        .iter()
        .find(|r| {
            matches!(
                r,
                StoreRef::LocalBookmark {
                    branch_name,
                    ..
                    } if branch_name == "immutable_bookmark"
            )
        })
        .unwrap();

    let working_copy = revs::working_copy();
    MoveRef {
        r#ref: immutable_bm.clone(),
        to_id: working_copy,
    }
    .execute_unboxed(&mut ws)
    .await?;

    let (_, after_change) = ws
        .view()
        .local_bookmarks_matching(&immutable_matcher)
        .next()
        .unwrap();
    let after_change = after_change.as_normal().unwrap().clone();

    // rev containing the bookmark is now immutable:
    assert_matches!(ws.check_immutable([after_change]), Ok(true));

    // checked-out rev is not immutable (because we made a new one):
    let current_ws_heads: Vec<jj_lib::backend::CommitId> = ws
        .repo()
        .view()
        .wc_commit_ids()
        .iter()
        .map(|(_, id)| id.clone())
        .collect();
    assert_matches!(ws.check_immutable(current_ws_heads), Ok(false));

    Ok(())
}

#[tokio::test]
async fn create_revision_single_parent() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let parent_header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
    assert!(parent_header.is_working_copy);

    let result = CreateRevision {
        parent_ids: vec![revs::working_copy()],
    }
    .execute_unboxed(&mut ws)
    .await?;

    match result {
        MutationResult::Updated {
            new_selection: Some(new_selection),
            ..
        } => {
            let parent_header =
                queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
            let child_header = queries::query_revision(&ws, &new_selection.id)?.expect("exists");
            assert!(!parent_header.is_working_copy);
            assert!(child_header.is_working_copy);
        }
        _ => panic!("CreateRevision failed"),
    }

    Ok(())
}

#[tokio::test]
async fn create_revision_multi_parent() -> Result<()> {
    let repo: tempfile::TempDir = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let parent_header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
    assert!(parent_header.is_working_copy);

    let result = CreateRevision {
        parent_ids: vec![revs::working_copy(), revs::conflict_bookmark()],
    }
    .execute_unboxed(&mut ws)
    .await?;

    match result {
        MutationResult::Updated {
            new_selection: Some(new_selection),
            ..
        } => {
            let child_rev = query_revision_details(&ws, new_selection.id).await?;
            assert_matches!(child_rev, RevsResult::Detail { parents, .. } if parents.len() == 2);
        }
        _ => panic!("CreateRevision failed"),
    }

    Ok(())
}

#[tokio::test]
async fn describe_revision() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
    assert!(header.description.lines[0].is_empty());

    let result = DescribeRevision {
        id: revs::working_copy(),
        new_description: "wip".to_owned(),
        reset_author: false,
    }
    .execute_unboxed(&mut ws)
    .await?;
    assert_matches!(result, MutationResult::Updated { .. });

    let header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
    assert_eq!(header.description.lines[0], "wip");

    Ok(())
}

#[tokio::test]
async fn describe_revision_with_snapshot() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let rev = query_revision_details(&ws, revs::working_copy()).await?;
    assert_matches!(rev, RevsResult::Detail { headers, changes, .. } if headers.last().unwrap().description.lines[0].is_empty() && changes.is_empty());

    fs::write(repo.path().join("new.txt"), []).unwrap(); // changes the WC commit

    DescribeRevision {
        id: revs::working_copy(),
        new_description: "wip".to_owned(),
        reset_author: false,
    }
    .execute_unboxed(&mut ws)
    .await?;

    let rev = query_revision_details(&ws, revs::working_copy()).await?;
    assert_matches!(rev, RevsResult::Detail { headers, changes, .. } if headers.last().unwrap().description.lines[0] == "wip" && !changes.is_empty());

    Ok(())
}

#[tokio::test]
async fn duplicate_revisions() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let header = queries::query_revision(&ws, &revs::working_copy())?.expect("exists");
    assert!(header.description.lines[0].is_empty());

    let result = DuplicateRevisions {
        ids: vec![revs::main_bookmark()],
    }
    .execute_unboxed(&mut ws)
    .await?;
    assert_matches!(result, MutationResult::Updated { .. });

    let page = queries::query_log(&ws, "description(unsynced)", 3)?;
    assert_eq!(2, page.rows.len());

    Ok(())
}

#[tokio::test]
async fn insert_revision() -> Result<()> {
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
    .execute_unboxed(&mut ws)
    .await?;

    let page = queries::query_log(&ws, "main::@", 4)?;
    assert_eq!(3, page.rows.len());

    Ok(())
}

#[tokio::test]
async fn move_changes_all_paths() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let parent_header = queries::query_revision(&ws, &revs::conflict_bookmark())?.expect("exists");
    assert!(parent_header.has_conflict);

    let result = MoveChanges {
        from_id: revs::resolve_conflict(),
        to_id: revs::conflict_bookmark().commit,
        paths: vec![],
    }
    .execute_unboxed(&mut ws)
    .await?;
    assert_matches!(result, MutationResult::Updated { .. });

    let parent_header = queries::query_revision(&ws, &revs::conflict_bookmark())?.expect("exists");
    assert!(!parent_header.has_conflict);

    Ok(())
}

#[tokio::test]
async fn move_changes_single_path() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let from_rev = query_revision_details(&ws, revs::main_bookmark()).await?;
    let to_rev = query_revision_details(&ws, revs::working_copy()).await?;
    assert_matches!(from_rev, RevsResult::Detail { changes, .. } if changes.len() == 2);
    assert_matches!(to_rev, RevsResult::Detail { changes, .. } if changes.is_empty());

    let result = MoveChanges {
        from_id: revs::main_bookmark(),
        to_id: revs::working_copy().commit,
        paths: vec![TreePath {
            repo_path: "c.txt".to_owned(),
            relative_path: "".into(),
        }],
    }
    .execute_unboxed(&mut ws)
    .await?;
    assert_matches!(result, MutationResult::Updated { .. });

    let from_rev = query_revision_details(&ws, revs::main_bookmark()).await?;
    let to_rev = query_revision_details(&ws, revs::working_copy()).await?;
    assert_matches!(from_rev, RevsResult::Detail { changes, .. } if changes.len() == 1);
    assert_matches!(to_rev, RevsResult::Detail { changes, .. } if changes.len() == 1);

    Ok(())
}

#[tokio::test]
async fn move_source() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let page = queries::query_log(&ws, "@+", 1)?;
    assert_eq!(0, page.rows.len());

    MoveSource {
        id: revs::resolve_conflict(),
        parent_ids: vec![revs::working_copy().commit],
    }
    .execute_unboxed(&mut ws)
    .await?;

    let page = queries::query_log(&ws, "@+", 2)?;
    assert_eq!(1, page.rows.len());

    Ok(())
}

#[tokio::test]
async fn move_hunk_descendant_partial() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Move one of two hunks from descendant (hunk_child_multi) to ancestor (hunk_base)
    // hunk_child_multi modifies lines 2 and 4: line2 -> changed2, line4 -> changed4
    // Move only the line 2 change to hunk_base, keeping line 4 change in source
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source still has the line 4 change but not line 2
    let source_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let source_tree = source_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match source_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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
    let target_tree = target_commit.tree();

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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

#[tokio::test]
async fn move_hunk_message() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Source should be abandoned (not found in repo)
    let source_header = queries::query_revision(&ws, &revs::hunk_child_single())?;
    assert!(source_header.is_none(), "Source should be abandoned");

    // Target should have combined description
    let target_header =
        queries::query_revision(&ws, &revs::hunk_sibling())?.expect("target exists");
    let desc = target_header.description.lines.join("\n");
    assert!(
        desc.contains("hunk sibling") && desc.contains("hunk child single"),
        "Target description should combine both: got '{}'",
        desc
    );

    Ok(())
}

#[tokio::test]
async fn move_hunk_invalid() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await;
    assert!(result.is_err(), "Should fail with invalid hunk");

    Ok(())
}

#[tokio::test]
async fn move_hunk_descendant_abandons_source() -> anyhow::Result<()> {
    use jj_lib::repo::Repo;

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

    // Move the only hunk from child to parent - child should be abandoned
    let mutation = MoveHunk {
        from_id: revs::hunk_child_single(),
        to_id: revs::hunk_base().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Source should be abandoned (not found in repo)
    let source_header = queries::query_revision(&ws, &revs::hunk_child_single())?;
    assert!(source_header.is_none(), "Source should be abandoned");

    // Target (hunk_base) should have the change
    let target_commit = get_rev(&ws, &revs::hunk_base())?;
    let target_tree = target_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nmodified2\nline3\nline4\nline5\n",
                "Target should have the hunk applied"
            );
        }
        _ => panic!("Expected hunk_test.txt to be a file in target commit"),
    }

    Ok(())
}

#[tokio::test]
async fn move_hunk_unrelated() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source has the hunk removed (becomes empty and should be abandoned or have no changes)
    let from_header = queries::query_revision(&ws, &revs::hunk_child_single())?;
    if from_header.is_some() {
        let from_rev = query_revision_details(&ws, revs::hunk_child_single()).await?;
        assert_matches!(from_rev, RevsResult::Detail { changes, .. } if changes.is_empty(),
            "Expected source commit to have no changes after hunk move");
    }

    // Verify target has the hunk applied (with the new lines still there)
    let sibling_commit = get_rev(&ws, &revs::hunk_sibling())?;
    let sibling_tree = sibling_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match sibling_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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

#[tokio::test]
async fn move_hunk_unrelated_different_structure_creates_conflict() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // The target should have a conflict because the file structures differ
    let to_header =
        queries::query_revision(&ws, &revs::working_copy())?.expect("working copy exists");
    assert!(
        to_header.has_conflict,
        "Expected conflict when moving hunk to file with different structure"
    );

    Ok(())
}

#[tokio::test]
async fn move_hunk_ancestor_to_descendant() -> anyhow::Result<()> {
    // Test moving a hunk FROM an ancestor TO a descendant (the from_is_ancestor code path).
    //
    // Hierarchy:
    //   hunk_base: line1, line2, line3, line4, line5
    //   └─ hunk_child_single: line1, modified2, line3, line4, line5  (changes line2)
    //      └─ hunk_grandchild: line1, modified2, grandchild3, line4, line5  (changes line3)
    //
    // We move the "line3 -> grandchild3" hunk FROM hunk_grandchild TO hunk_child_single.
    // Wait, that's descendant-to-ancestor which is already tested.
    //
    // For a true ancestor-to-descendant test without causing source abandonment,
    // we use hunk_child_multi (which has 2 hunks) and move one hunk to hunk_sibling.
    // But they're siblings, not ancestor-descendant.
    //
    // Actually, let's test moving hunk_child_single's change to hunk_grandchild.
    // When the source's only change is moved, the source is abandoned and the
    // grandchild is rebased. This verifies the rebase-and-apply logic.
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    // Verify initial state
    let child_before = get_rev(&ws, &revs::hunk_child_single())?;
    let child_tree_before = child_before.tree();
    match child_tree_before.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            assert_eq!(
                String::from_utf8_lossy(&content),
                "line1\nmodified2\nline3\nline4\nline5\n",
                "hunk_child_single initial state"
            );
        }
        _ => panic!("Expected hunk_test.txt in hunk_child_single"),
    }

    let grandchild_before = get_rev(&ws, &revs::hunk_grandchild())?;
    let grandchild_tree_before = grandchild_before.tree();
    match grandchild_tree_before
        .path_value(&repo_path)?
        .into_resolved()
    {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            assert_eq!(
                String::from_utf8_lossy(&content),
                "line1\nmodified2\ngrandchild3\nline4\nline5\n",
                "hunk_grandchild initial state"
            );
        }
        _ => panic!("Expected hunk_test.txt in hunk_grandchild"),
    }

    // The hunk in hunk_child_single's context: parent (hunk_base) has line2, child has modified2
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

    // Move FROM ancestor (hunk_child_single) TO descendant (hunk_grandchild)
    let mutation = MoveHunk {
        from_id: revs::hunk_child_single(),
        to_id: revs::hunk_grandchild().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source (hunk_child_single) was ABANDONED because its only change was moved
    let source_header = queries::query_revision(&ws, &revs::hunk_child_single())?;
    assert!(
        source_header.is_none(),
        "Source should be abandoned (its only change was moved)"
    );

    // Verify destination (hunk_grandchild) - should have the hunk applied correctly
    let dest_after = get_rev(&ws, &revs::hunk_grandchild())?;
    let dest_tree = dest_after.tree();

    let path_value = dest_tree.path_value(&repo_path)?;
    match path_value.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nmodified2\ngrandchild3\nline4\nline5\n",
                "Destination should have modified2 (moved) and grandchild3 (own)"
            );
        }
        Ok(None) => panic!("hunk_test.txt does not exist in destination"),
        Ok(other) => panic!("hunk_test.txt has unexpected type: {:?}", other),
        Err(_conflict) => {
            panic!("Destination should not have a conflict after move");
        }
    }

    // Verify grandchild is now a direct child of hunk_base (parent was abandoned)
    let grandchild_parents: Vec<_> = dest_after.parents().collect();
    assert_eq!(
        grandchild_parents.len(),
        1,
        "Grandchild should have one parent"
    );
    let parent = grandchild_parents[0].as_ref().unwrap();
    let base = get_rev(&ws, &revs::hunk_base())?;
    assert_eq!(
        parent.id(),
        base.id(),
        "Grandchild's parent should now be hunk_base (skipping abandoned hunk_child_single)"
    );

    Ok(())
}

#[tokio::test]
async fn move_hunk_between_siblings() -> anyhow::Result<()> {
    // Test moving hunks between sibling commits (both children of same parent).
    // This exercises the general code path where neither commit is ancestor of the other.
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // hunk_child_multi has: line1, changed2, line3, changed4, line5
    // hunk_sibling has: line1, line2, line3, line4, line5, new6, new7, new8
    // Both are children of hunk_base (line1, line2, line3, line4, line5)
    //
    // Move the line2->changed2 hunk from hunk_child_multi to hunk_sibling
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
        to_id: revs::hunk_sibling().commit,
        path: TreePath {
            repo_path: "hunk_test.txt".to_owned(),
            relative_path: "".into(),
        },
        hunk,
    };

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source has the hunk removed (only changed4 remains)
    let source_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let source_tree = source_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match source_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nline2\nline3\nchanged4\nline5\n",
                "Source should have only changed4 (changed2 was moved)"
            );
        }
        _ => panic!("Expected hunk_test.txt in source"),
    }

    // Verify target has both the new lines AND the moved hunk
    let target_commit = get_rev(&ws, &revs::hunk_sibling())?;
    let target_tree = target_commit.tree();

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            let content_str = String::from_utf8_lossy(&content);
            assert_eq!(
                content_str, "line1\nchanged2\nline3\nline4\nline5\nnew6\nnew7\nnew8\n",
                "Target should have changed2 (moved) plus its own new lines"
            );
        }
        _ => panic!("Expected hunk_test.txt in target"),
    }

    Ok(())
}

#[tokio::test]
async fn move_hunk_does_not_affect_other_files() -> anyhow::Result<()> {
    // Verify that moving a hunk in one file doesn't affect other files in the same commits.
    use jj_lib::repo::Repo;

    let repo = mkrepo();
    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Get the original content of a.txt in hunk_child_multi before the move
    let child_before = get_rev(&ws, &revs::hunk_child_multi())?;
    let child_tree_before = child_before.tree();
    let a_txt_path = jj_lib::repo_path::RepoPath::from_internal_string("a.txt")?;

    let a_txt_content_before = match child_tree_before.path_value(&a_txt_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&a_txt_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            String::from_utf8_lossy(&content).to_string()
        }
        Ok(None) => String::new(), // File doesn't exist
        _ => panic!("Unexpected state for a.txt"),
    };

    // Also check hunk_base's a.txt content
    let parent_before = get_rev(&ws, &revs::hunk_base())?;
    let parent_tree_before = parent_before.tree();

    let parent_a_txt_before = match parent_tree_before.path_value(&a_txt_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&a_txt_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            String::from_utf8_lossy(&content).to_string()
        }
        Ok(None) => String::new(),
        _ => panic!("Unexpected state for a.txt in parent"),
    };

    // Move a hunk in hunk_test.txt from child to parent
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify a.txt in child is unchanged
    let child_after = get_rev(&ws, &revs::hunk_child_multi())?;
    let child_tree_after = child_after.tree();

    let a_txt_content_after = match child_tree_after.path_value(&a_txt_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&a_txt_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            String::from_utf8_lossy(&content).to_string()
        }
        Ok(None) => String::new(),
        _ => panic!("Unexpected state for a.txt after move"),
    };

    assert_eq!(
        a_txt_content_before, a_txt_content_after,
        "a.txt in child should be unchanged after moving hunk in hunk_test.txt"
    );

    // Verify a.txt in parent is unchanged
    let parent_after = get_rev(&ws, &revs::hunk_base())?;
    let parent_tree_after = parent_after.tree();

    let parent_a_txt_after = match parent_tree_after.path_value(&a_txt_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&a_txt_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            String::from_utf8_lossy(&content).to_string()
        }
        Ok(None) => String::new(),
        _ => panic!("Unexpected state for a.txt in parent after move"),
    };

    assert_eq!(
        parent_a_txt_before, parent_a_txt_after,
        "a.txt in parent should be unchanged after moving hunk in hunk_test.txt"
    );

    Ok(())
}

#[tokio::test]
async fn copy_hunk_from_parent() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify: child should now have parent's content (restoration)
    let child_commit = get_rev(&ws, &revs::hunk_child_single())?;
    let child_tree = child_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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

#[tokio::test]
async fn copy_hunk_to_conflict() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await;

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

#[tokio::test]
async fn copy_hunk_out_of_bounds() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await;

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

#[tokio::test]
async fn copy_hunk_unchanged() -> anyhow::Result<()> {
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
    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Unchanged);

    Ok(())
}

#[tokio::test]
async fn copy_hunk_multiple_hunks() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify: line 2 still modified (changed2), line 4 restored (line4)
    let child_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let child_tree = child_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match child_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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

#[tokio::test]
async fn move_hunk_second_of_two_hunks() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws).await?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Verify source still has the first hunk (changed2), but not the second
    let source_commit = get_rev(&ws, &revs::hunk_child_multi())?;
    let source_tree = source_commit.tree();
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match source_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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
    let target_tree = target_commit.tree();

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = ws.repo().store().read_file(&repo_path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
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

// XXX possibly this should be a session test using the ExecuteSnapshot event
#[tokio::test]
async fn snapshot_respects_auto_track_config() -> Result<()> {
    let repo = mkrepo();

    // Configure snapshot.auto-track to only track .txt files
    let config_path = repo.path().join(".jj/repo/config.toml");
    let config_content = r#"
[snapshot]
auto-track = "glob:*.txt"
"#;
    fs::write(&config_path, config_content).unwrap();

    let mut session = WorkerSession::default();
    let mut ws = session.load_directory(repo.path())?;

    // Write two new files, one tracked and one untracked
    fs::write(repo.path().join("tracked.txt"), "tracked content").unwrap();
    fs::write(repo.path().join("untracked.dat"), "untracked content").unwrap();

    // Trigger a snapshot by describing the revision
    DescribeRevision {
        id: revs::working_copy(),
        new_description: "test auto-track".to_owned(),
        reset_author: false,
    }
    .execute_unboxed(&mut ws)
    .await?;

    // Verify: only the .txt file should have been tracked
    let rev = query_revision_details(&ws, revs::working_copy()).await?;
    match rev {
        RevsResult::Detail { changes, .. } => {
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].path.repo_path, "tracked.txt");
        }
        _ => panic!("Expected working copy to exist"),
    }

    // Verify: the .dat file should exist, but be untracked
    assert!(repo.path().join("untracked.dat").exists());

    Ok(())
}

// XXX missing tests for:
// - git interop
