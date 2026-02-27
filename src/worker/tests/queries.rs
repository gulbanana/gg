use super::{mkid, mkrepo, revs};
use crate::messages::{RevSet, RevsResult, StoreRef};
use crate::worker::{WorkerSession, queries};
use anyhow::Result;
use assert_matches::assert_matches;
use std::collections::HashSet;

#[test]
fn log_all() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let all_rows = queries::query_log(&ws, "all()", 100)?;

    assert_eq!(24, all_rows.rows.len());
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

    let single_row = queries::query_log(&ws, "wnpusytq", 100)?
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

    let single_row = queries::query_log(&ws, "ywknyuol", 100)?
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

    let header = queries::query_revision(&ws, &revs::main_bookmark())?.expect("revision exists");

    assert_matches!(
        header.refs.as_slice(),
        [StoreRef::LocalBookmark { bookmark_name, .. }] if bookmark_name == "main"
    );

    Ok(())
}

/// Test that querying a conflicted revision includes conflict markers in the hunks.
/// The conflict labels from the trees should be passed through to materialize_tree_value().
#[tokio::test]
async fn revision_with_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let id = revs::conflict_bookmark();
    let result = queries::query_revisions(
        &ws,
        RevSet {
            from: id.clone(),
            to: id,
        },
    )
    .await?;

    let RevsResult::Detail {
        headers, conflicts, ..
    } = result
    else {
        panic!("Expected RevsResult::Detail");
    };

    let header = headers.last().expect("at least one header");

    // The conflict_bookmark commit should be marked as having conflicts
    assert!(
        header.has_conflict,
        "Expected header.has_conflict to be true"
    );

    // The conflicts field should contain the conflict info from the final tree
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

#[tokio::test]
async fn conflicted_paths_are_not_duplicated() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let result = queries::query_revisions(
        &ws,
        RevSet {
            from: revs::hunk_source(),
            to: revs::inherited_conflict(),
        },
    )
    .await?;

    let RevsResult::Detail {
        changes, conflicts, ..
    } = result
    else {
        panic!("Expected RevsResult::Detail");
    };

    let conflict_paths: HashSet<String> = conflicts
        .into_iter()
        .map(|conflict| conflict.path.repo_path)
        .collect();
    let duplicated_paths: Vec<String> = changes
        .into_iter()
        .map(|change| change.path.repo_path)
        .filter(|path| conflict_paths.contains(path))
        .collect();

    assert!(
        duplicated_paths.is_empty(),
        "Expected conflicted paths to appear once, duplicates: {:?}",
        duplicated_paths
    );

    Ok(())
}

/// Test that querying a revision without conflicts returns an empty conflicts list.
#[tokio::test]
async fn revision_without_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    let id = revs::main_bookmark();
    let result = queries::query_revisions(
        &ws,
        RevSet {
            from: id.clone(),
            to: id,
        },
    )
    .await?;

    let RevsResult::Detail {
        headers, conflicts, ..
    } = result
    else {
        panic!("Expected RevsResult::Detail");
    };

    let header = headers.last().expect("at least one header");

    assert!(
        !header.has_conflict,
        "Expected header.has_conflict to be false"
    );

    assert!(
        conflicts.is_empty(),
        "Expected conflicts to be empty for non-conflicted revision"
    );

    Ok(())
}

/// Test that resolving a conflict results in no conflicts in the final tree.
/// The diff shows removal of labeled conflict markers from the parent tree.
#[tokio::test]
async fn revision_resolves_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    // resolve_conflict is a child of conflict_bookmark that resolves the conflict
    let id = revs::resolve_conflict();
    let result = queries::query_revisions(
        &ws,
        RevSet {
            from: id.clone(),
            to: id,
        },
    )
    .await?;

    let RevsResult::Detail {
        headers,
        changes,
        conflicts,
        ..
    } = result
    else {
        panic!("Expected RevsResult::Detail");
    };

    let header = headers.last().expect("at least one header");

    // This commit resolved the conflict, so it should not be conflicted
    assert!(
        !header.has_conflict,
        "Expected header.has_conflict to be false for resolved commit"
    );

    // The conflicts field should be empty since the final tree has no conflicts
    assert!(
        conflicts.is_empty(),
        "Expected conflicts to be empty for resolved commit, got {} conflicts",
        conflicts.len()
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

#[tokio::test]
async fn revisions_nonexistent_range_returns_not_found() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    // use fake change IDs that don't exist in the repo
    let nonexistent_set = RevSet {
        from: mkid("aaaaaaaa", "0000000000000000000000000000000000000000"),
        to: mkid("bbbbbbbb", "1111111111111111111111111111111111111111"),
    };

    let result = queries::query_revisions(&ws, nonexistent_set).await?;
    assert_matches!(
        result,
        RevsResult::NotFound { .. },
        "Querying non-existent range should return NotFound"
    );

    Ok(())
}

/// Test that a merge commit introduces a new conflict where neither parent was conflicted.
/// This verifies the "before" side of the diff (parent trees) has no conflict markers.
#[tokio::test]
async fn merge_introduces_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    // First verify that a parent of the conflict commit has no conflict
    let parent_id = revs::hunk_source(); // xoooutru - one parent of conflict_bookmark
    let parent_result = queries::query_revisions(
        &ws,
        RevSet {
            from: parent_id.clone(),
            to: parent_id,
        },
    )
    .await?;

    let RevsResult::Detail {
        headers: parent_headers,
        conflicts: parent_conflicts,
        ..
    } = parent_result
    else {
        panic!("Expected RevsResult::Detail for parent");
    };

    let parent_header = parent_headers.last().expect("at least one header");
    assert!(
        !parent_header.has_conflict,
        "Parent commit should not have conflict"
    );
    assert!(
        parent_conflicts.is_empty(),
        "Parent commit should have empty conflicts list"
    );

    // Now query the merge commit that introduces the conflict
    let merge_id = revs::conflict_bookmark();
    let merge_result = queries::query_revisions(
        &ws,
        RevSet {
            from: merge_id.clone(),
            to: merge_id,
        },
    )
    .await?;

    let RevsResult::Detail {
        headers: merge_headers,
        changes,
        conflicts,
        ..
    } = merge_result
    else {
        panic!("Expected RevsResult::Detail for merge");
    };

    let merge_header = merge_headers.last().expect("at least one header");
    assert!(
        merge_header.has_conflict,
        "Merge commit should have conflict"
    );
    assert!(
        !conflicts.is_empty(),
        "Merge commit should have non-empty conflicts"
    );

    // The diff shows changes FROM parent TO merge commit
    // Since the parent wasn't conflicted, the "before" side should NOT have conflict markers
    // Only the "after" side (merge tree) has conflicts, which appear in the `conflicts` field
    let all_change_lines: String = changes
        .iter()
        .flat_map(|c| &c.hunks)
        .flat_map(|h| &h.lines.lines)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    // The changes diff should NOT contain conflict markers in the "before" (deleted) lines
    // because the parent trees weren't conflicted
    let has_conflict_in_before = all_change_lines.contains("-<<<<<<<")
        || all_change_lines.contains("->>>>>>>")
        || all_change_lines.contains("-+++++++")
        || all_change_lines.contains("--------");

    assert!(
        !has_conflict_in_before,
        "Parent trees should not have conflict markers in diff:\n{all_change_lines}"
    );

    Ok(())
}

/// Test that conflicts are inherited through a range when not resolved.
/// A child commit that doesn't resolve a conflict should still show the conflict.
#[tokio::test]
async fn inherited_conflict_persists() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    // Query the child that inherits but doesn't resolve the conflict
    let child_id = revs::inherited_conflict();
    let result = queries::query_revisions(
        &ws,
        RevSet {
            from: child_id.clone(),
            to: child_id,
        },
    )
    .await?;

    let RevsResult::Detail {
        headers,
        changes,
        conflicts,
        ..
    } = result
    else {
        panic!("Expected RevsResult::Detail");
    };

    let header = headers.last().expect("at least one header");

    // The inherited conflict commit should still be marked as conflicted
    assert!(
        header.has_conflict,
        "Expected header.has_conflict to be true for inherited conflict"
    );

    // Conflicts should be non-empty since b.txt is still conflicted
    assert!(
        !conflicts.is_empty(),
        "Expected conflicts to be non-empty for inherited conflict"
    );

    // The conflict markers should still be present
    let conflict_lines: String = conflicts
        .iter()
        .flat_map(|c| &c.hunk.lines.lines)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        conflict_lines.contains("<<<<<<<") && conflict_lines.contains(">>>>>>>"),
        "Expected conflict markers in inherited conflict, got: {conflict_lines}"
    );

    // There should be a change for the unrelated.txt file that was added
    let has_unrelated_change = changes
        .iter()
        .any(|c| c.path.repo_path.ends_with("unrelated.txt"));
    assert!(
        has_unrelated_change,
        "Expected change for unrelated.txt in inherited conflict commit"
    );

    Ok(())
}

/// Test that a range through inherited conflicts shows the final tree's conflict state.
#[tokio::test]
async fn range_through_inherited_conflict() -> Result<()> {
    let repo = mkrepo();

    let mut session = WorkerSession::default();
    let ws = session.load_directory(repo.path())?;

    // Query range from parent (conflict_bookmark) to child (inherited_conflict)
    let result = queries::query_revisions(
        &ws,
        RevSet {
            from: revs::conflict_bookmark(),
            to: revs::inherited_conflict(),
        },
    )
    .await?;

    let RevsResult::Detail {
        headers, conflicts, ..
    } = result
    else {
        panic!("Expected RevsResult::Detail");
    };

    // Should have 2 commits in range
    assert_eq!(headers.len(), 2, "Expected 2 headers in range");

    // Both commits should be marked as conflicted
    assert!(
        headers.iter().all(|h| h.has_conflict),
        "All commits in range should have conflicts"
    );

    // The conflicts list reflects the final tree (inherited_conflict)
    // which still has the conflict in b.txt
    assert!(
        !conflicts.is_empty(),
        "Range ending in conflicted commit should have conflicts"
    );

    Ok(())
}

/// Test multi-revision ranges where conflicts are introduced and resolved.
/// The chain is: resolve_conflict (no conflict) -> chain_conflict (conflict) -> chain_resolved (no conflict)
mod conflict_chain_ranges {
    use super::*;

    /// Range ending in a conflicted commit should show conflicts
    #[tokio::test]
    async fn range_ends_in_conflict() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // Range from resolve_conflict to chain_conflict
        // resolve_conflict has no conflicts, chain_conflict does
        let result = queries::query_revisions(
            &ws,
            RevSet {
                from: revs::resolve_conflict(),
                to: revs::chain_conflict(),
            },
        )
        .await?;

        let RevsResult::Detail {
            headers, conflicts, ..
        } = result
        else {
            panic!("Expected RevsResult::Detail");
        };

        // Final commit (chain_conflict) has conflict
        let final_header = headers.first().expect("at least one header");
        assert!(
            final_header.has_conflict,
            "Final commit in range should have conflict"
        );

        // Conflicts field reflects final tree state
        assert!(
            !conflicts.is_empty(),
            "Range ending in conflicted commit should have non-empty conflicts"
        );

        // Verify it's the conflict_chain.txt conflict
        let conflict_paths: Vec<_> = conflicts.iter().map(|c| &c.path.repo_path).collect();
        assert!(
            conflict_paths.iter().any(|p| p.contains("conflict_chain")),
            "Expected conflict in conflict_chain.txt, got: {:?}",
            conflict_paths
        );

        Ok(())
    }

    /// Range starting with conflict, ending with resolution should show no conflicts
    #[tokio::test]
    async fn range_conflict_to_resolved() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // Range from chain_conflict to chain_resolved
        let result = queries::query_revisions(
            &ws,
            RevSet {
                from: revs::chain_conflict(),
                to: revs::chain_resolved(),
            },
        )
        .await?;

        let RevsResult::Detail {
            headers, conflicts, ..
        } = result
        else {
            panic!("Expected RevsResult::Detail");
        };

        assert_eq!(headers.len(), 2, "Should have 2 headers in range");

        // Oldest commit (chain_conflict) has conflict
        let oldest = headers.last().expect("at least one header");
        assert!(oldest.has_conflict, "Oldest commit should have conflict");

        // Newest commit (chain_resolved) has no conflict
        let newest = headers.first().expect("at least one header");
        assert!(
            !newest.has_conflict,
            "Newest commit should not have conflict"
        );

        // Conflicts field reflects final tree (chain_resolved) which has no conflicts
        assert!(
            conflicts.is_empty(),
            "Range ending in resolved commit should have empty conflicts"
        );

        Ok(())
    }

    /// Long range through multiple conflict states
    /// conflict_bookmark (conflict) -> resolve_conflict (no) -> chain_conflict (conflict) -> chain_resolved (no)
    #[tokio::test]
    async fn range_through_multiple_conflict_states() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // Range from original conflict through to final resolution
        let result = queries::query_revisions(
            &ws,
            RevSet {
                from: revs::conflict_bookmark(),
                to: revs::chain_resolved(),
            },
        )
        .await?;

        let RevsResult::Detail {
            headers, conflicts, ..
        } = result
        else {
            panic!("Expected RevsResult::Detail");
        };

        // Should have multiple commits in range
        assert!(
            headers.len() >= 4,
            "Expected at least 4 headers, got {}",
            headers.len()
        );

        // Final tree (chain_resolved) has no conflicts
        assert!(
            conflicts.is_empty(),
            "Range ending in fully resolved commit should have empty conflicts"
        );

        // Verify we have mixed conflict states in headers
        let conflict_states: Vec<bool> = headers.iter().map(|h| h.has_conflict).collect();
        let has_conflicted = conflict_states.iter().any(|&c| c);
        let has_resolved = conflict_states.iter().any(|&c| !c);
        assert!(
            has_conflicted && has_resolved,
            "Range should have both conflicted and resolved commits: {:?}",
            conflict_states
        );

        Ok(())
    }

    /// Verify single conflicted commit in chain
    #[tokio::test]
    async fn single_chain_conflict() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        let id = revs::chain_conflict();
        let result = queries::query_revisions(
            &ws,
            RevSet {
                from: id.clone(),
                to: id,
            },
        )
        .await?;

        let RevsResult::Detail {
            headers, conflicts, ..
        } = result
        else {
            panic!("Expected RevsResult::Detail");
        };

        let header = headers.last().expect("at least one header");
        assert!(header.has_conflict, "chain_conflict should have conflict");
        assert!(
            !conflicts.is_empty(),
            "chain_conflict should have non-empty conflicts"
        );

        Ok(())
    }

    /// Verify single resolved commit in chain
    #[tokio::test]
    async fn single_chain_resolved() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        let id = revs::chain_resolved();
        let result = queries::query_revisions(
            &ws,
            RevSet {
                from: id.clone(),
                to: id,
            },
        )
        .await?;

        let RevsResult::Detail {
            headers, conflicts, ..
        } = result
        else {
            panic!("Expected RevsResult::Detail");
        };

        let header = headers.last().expect("at least one header");
        assert!(
            !header.has_conflict,
            "chain_resolved should not have conflict"
        );
        assert!(
            conflicts.is_empty(),
            "chain_resolved should have empty conflicts"
        );

        Ok(())
    }
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

mod revisions_immutability {
    //! Tests for query_revisions immutability checking.
    //!
    //! The test repository has a linear chain: root -> ... -> immutable_grandparent ->
    //! immutable_parent -> immutable_bookmark -> main_bookmark -> working_copy
    //!
    //! immutable_bookmark and its ancestors are immutable; main_bookmark and working_copy are mutable.

    use super::*;
    use crate::messages::{RevSet, RevsResult};

    /// Helper to create a RevSet from two RevIds
    fn mkset(from: crate::messages::RevId, to: crate::messages::RevId) -> RevSet {
        RevSet { from, to }
    }

    /// Helper to extract immutability flags from query result
    fn get_immutability(result: &RevsResult) -> Vec<bool> {
        match result {
            RevsResult::Detail { headers, .. } => headers.iter().map(|h| h.is_immutable).collect(),
            RevsResult::NotFound { .. } => panic!("Expected Detail, got NotFound"),
        }
    }

    #[tokio::test]
    async fn single_revision_immutable() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        let set = mkset(revs::immutable_bookmark(), revs::immutable_bookmark());
        let result = queries::query_revisions(&ws, set).await?;

        let flags = get_immutability(&result);
        assert_eq!(
            flags,
            vec![true],
            "Single immutable revision should be marked immutable"
        );

        Ok(())
    }

    #[tokio::test]
    async fn single_revision_mutable() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        let set = mkset(revs::main_bookmark(), revs::main_bookmark());
        let result = queries::query_revisions(&ws, set).await?;

        let flags = get_immutability(&result);
        assert_eq!(
            flags,
            vec![false],
            "Single mutable revision should be marked mutable"
        );

        Ok(())
    }

    #[tokio::test]
    async fn sequence_all_immutable() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // immutable_grandparent -> immutable_parent -> immutable_bookmark (all immutable)
        let set = mkset(revs::immutable_grandparent(), revs::immutable_bookmark());
        let result = queries::query_revisions(&ws, set).await?;

        let flags = get_immutability(&result);
        assert_eq!(flags.len(), 3, "Should have 3 revisions in range");
        assert!(
            flags.iter().all(|&f| f),
            "All revisions in immutable range should be immutable: {:?}",
            flags
        );

        Ok(())
    }

    #[tokio::test]
    async fn sequence_all_mutable() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // main_bookmark -> working_copy (both mutable)
        let set = mkset(revs::main_bookmark(), revs::working_copy());
        let result = queries::query_revisions(&ws, set).await?;

        let flags = get_immutability(&result);
        assert_eq!(flags.len(), 2, "Should have 2 revisions in range");
        assert!(
            flags.iter().all(|&f| !f),
            "All revisions in mutable range should be mutable: {:?}",
            flags
        );

        Ok(())
    }

    #[tokio::test]
    async fn sequence_oldest_immutable_newest_mutable() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // immutable_bookmark -> main_bookmark (oldest immutable, newest mutable)
        let set = mkset(revs::immutable_bookmark(), revs::main_bookmark());
        let result = queries::query_revisions(&ws, set).await?;

        let flags = get_immutability(&result);
        assert_eq!(flags.len(), 2, "Should have 2 revisions in range");
        assert_eq!(
            flags,
            vec![false, true],
            "Oldest should be immutable, newest should be mutable"
        );

        Ok(())
    }

    #[tokio::test]
    async fn sequence_mixed_immutability_longer() -> Result<()> {
        let repo = mkrepo();
        let mut session = WorkerSession::default();
        let ws = session.load_directory(repo.path())?;

        // immutable_parent -> immutable_bookmark -> main_bookmark -> working_copy
        // First two immutable, last two mutable
        let set = mkset(revs::immutable_parent(), revs::working_copy());
        let result = queries::query_revisions(&ws, set).await?;

        let flags = get_immutability(&result);
        assert_eq!(flags.len(), 4, "Should have 4 revisions in range");
        assert_eq!(
            flags,
            vec![false, false, true, true],
            "First two should be immutable, last two should be mutable"
        );

        Ok(())
    }
}
