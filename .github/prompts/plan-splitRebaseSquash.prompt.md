# Implementation Plan: Split-Rebase-Squash Algorithm for MoveHunk

## Goal

Simplify and unify the `MoveHunk` implementation by restructuring the descendant-to-ancestor case to use `rebase_descendants()` like `jj squash` does, removing the manual tree recomputation and enabling the removal of the `abandon_source && to_is_ancestor` precondition.

## Background

### Current Algorithm

The current `MoveHunk` implementation in `src-tauri/src/worker/mutations.rs` (lines 896-1050) uses 3-way merge for both removing the hunk from source and applying it to destination:

```rust
// Current approach:
let remainder_tree = from_tree.merge(hunk_tree, parent_tree).await?;  // Remove from source
let new_to_tree = to_tree.merge(parent_tree, hunk_tree).await?;       // Apply to destination
```

This matches `jj squash` behavior, which also uses 3-way merge:
```rust
// From jj-lib squash_commits:
destination_tree = destination_tree.merge(parent_tree, selected_tree).block_on()?;
```

### The Problem: Descendant-to-Ancestor Case

The current implementation has a complex special case for moving hunks from a descendant to an ancestor (lines 968-987):

```rust
if to_is_ancestor {
    let new_to = tx.repo_mut().rewrite_commit(&to).set_tree_id(new_to_tree.id()).write()?;
    
    // Manual tree recomputation - this is the complex part
    let child_new_tree = new_to_tree.clone().merge(parent_tree, from_tree).await?;
    
    tx.repo_mut().rewrite_commit(&from)
        .set_parents(vec![new_to.id().clone()])
        .set_tree_id(child_new_tree.id().clone())
        .write()?;
}
```

This manual tree recomputation can create conflicts because:
1. `new_to_tree` has the hunk applied
2. `from_tree` also has the hunk (plus other changes)
3. The merge tries to reconcile these, potentially creating conflicts

Additionally, there's a precondition that blocks the case when the source would be abandoned:
```rust
if abandon_source && to_is_ancestor {
    precondition!("Moving a hunk from a commit that becomes empty to an ancestor is not supported");
}
```

### How jj squash Handles This

Looking at `jj-lib`'s `squash_commits` (from `rewrite.rs`), it doesn't have special tree recomputation. Instead:

1. Rewrite the source with `remainder_tree` (hunk removed)
2. If source is abandoned, call `record_abandoned_commit()`
3. Call `rebase_descendants()` to handle all descendant updates automatically
4. Apply the hunk to destination using 3-way merge

The key insight is that `rebase_descendants()` handles the tree reconciliation automatically - we don't need to manually compute what the rebased source's tree should look like.

### Reference: How MoveChanges Does It

`MoveChanges` (lines 425-498) uses this simpler pattern:

```rust
// Abandon or rewrite source
if abandon_source {
    tx.repo_mut().record_abandoned_commit(&from);
} else {
    tx.repo_mut().rewrite_commit(&from).set_tree_id(remainder_tree.id()).write()?;
}

// Rebase descendants (handles ancestor-to-descendant case)
if tx.repo().index().is_ancestor(from.id(), to.id())? {
    // ... rebase and update `to` reference
}

// Apply to destination
let new_to_tree = to_tree.merge(parent_tree, split_tree).await?;
tx.repo_mut().rewrite_commit(&to).set_tree_id(new_to_tree.id()).write()?;
```

Note: `MoveChanges` doesn't handle the descendant-to-ancestor case at all. We need to add that handling.

## Files to Modify

1. `src-tauri/src/worker/mutations.rs` - Main implementation changes
2. `src-tauri/src/worker/tests/mutations.rs` - Test updates and new tests

## Implementation Steps

### Step 1: Remove the Precondition

**Location**: `src-tauri/src/worker/mutations.rs`, lines ~962-966

**Current code**:
```rust
let abandon_source = remainder_tree.id() == parent_tree.id();

// block moving a hunk from a commit that becomes empty to an ancestor
// (this would require abandoning a commit while also rebasing it)
if abandon_source && to_is_ancestor {
    precondition!(
        "Moving a hunk from a commit that becomes empty to an ancestor is not supported"
    );
}
```

**New code**:
```rust
let abandon_source = remainder_tree.id() == parent_tree.id();
```

Simply remove the precondition block. Testing will verify whether this works correctly.

### Step 2: Restructure Descendant-to-Ancestor Branch

**Location**: `src-tauri/src/worker/mutations.rs`, lines ~968-987

**Current code**:
```rust
if to_is_ancestor {
    // special case: descendant-to-ancestor
    // Must modify ancestor first, then rebase descendant to pick up inherited changes

    let new_to = tx
        .repo_mut()
        .rewrite_commit(&to)
        .set_tree_id(new_to_tree.id().clone())
        .set_description(description)
        .write()?;

    // recompute the source's tree after the destination has the hunk applied
    let child_new_tree = new_to_tree.clone().merge(parent_tree, from_tree).await?;

    // rebase source onto modified ancestor
    tx.repo_mut()
        .rewrite_commit(&from)
        .set_parents(vec![new_to.id().clone()])
        .set_tree_id(child_new_tree.id().clone())
        .write()?;
}
```

**New code**:
```rust
if to_is_ancestor {
    // Descendant-to-ancestor: modify ancestor first, then rewrite/abandon source
    // rebase_descendants() will handle tree reconciliation automatically

    tx.repo_mut()
        .rewrite_commit(&to)
        .set_tree_id(new_to_tree.id().clone())
        .set_description(description)
        .write()?;

    // Abandon or rewrite source
    if abandon_source {
        tx.repo_mut().record_abandoned_commit(&from);
    } else {
        tx.repo_mut()
            .rewrite_commit(&from)
            .set_tree_id(remainder_tree.id().clone())
            .write()?;
    }

    // Let jj handle rebasing descendants onto the modified ancestor
    tx.repo_mut().rebase_descendants()?;
}
```

**Key changes**:
1. Remove manual tree recomputation (`child_new_tree = new_to_tree.merge(...)`)
2. Handle `abandon_source` case (now allowed since precondition is removed)
3. Use `rebase_descendants()` instead of manual `set_parents()` + `set_tree_id()`

### Step 3: Update Comment for Clarity

**Location**: `src-tauri/src/worker/mutations.rs`, around line 946

**Current comment**:
```rust
// use 3-way merge for all cases (equivalent to split-then-squash)
// - remove hunk from source: from_tree.merge(hunk_tree, parent_tree) backs out the hunk
// - apply hunk to destination: to_tree.merge(parent_tree, hunk_tree) applies the hunk
```

**New comment**:
```rust
// Split-rebase-squash algorithm (matches jj squash behavior):
// 1. hunk_tree is a "sibling" of from: it's parent_tree with just the hunk applied
// 2. remainder_tree removes the hunk from source via 3-way backout (deterministic)
// 3. new_to_tree applies the hunk to destination via 3-way merge
// 4. rebase_descendants() handles tree reconciliation for related commits
```

### Step 4: Update Test - move_hunk_descendant

**Location**: `src-tauri/src/worker/tests/mutations.rs`, lines ~507-548

**Current test**: Named `move_hunk_descendant`, expects `PreconditionError` when moving the only hunk from child to ancestor.

**Rename to**: `move_hunk_descendant_abandons_source`

**New expected behavior**: Success, with the child being abandoned.

```rust
#[test]
fn move_hunk_descendant_abandons_source() -> anyhow::Result<()> {
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

    let result = mutation.execute_unboxed(&mut ws)?;
    assert_matches!(result, MutationResult::Updated { .. });

    // Source should be abandoned
    let source_rev = queries::query_revision(&ws, revs::hunk_child_single())?;
    assert_matches!(source_rev, RevResult::NotFound { .. }, "Source should be abandoned");

    // Target (hunk_base) should have the change
    let target_commit = get_rev(&ws, &revs::hunk_base())?;
    let target_tree = target_commit.tree()?;
    let repo_path = jj_lib::repo_path::RepoPath::from_internal_string("hunk_test.txt")?;

    match target_tree.path_value(&repo_path)?.into_resolved() {
        Ok(Some(jj_lib::backend::TreeValue::File { id, .. })) => {
            let mut reader = block_on(ws.repo().store().read_file(&repo_path, &id))?;
            let mut content = Vec::new();
            block_on(reader.read_to_end(&mut content))?;
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
```

### Step 5: Review Existing Test - move_hunk_basic

**Location**: `src-tauri/src/worker/tests/mutations.rs`, lines ~349-411

The existing `move_hunk_basic` test moves a hunk from `hunk_child_multi` to `hunk_base` (descendant to ancestor). This test should continue to pass and verifies the partial move (non-abandon) case:

- The source should have `changed2` (inherited from rebased parent) and `changed4` (its own)
- The target should have `changed2` only

If this test still passes after the changes, we don't need to add `move_hunk_descendant_partial` separately.

### Step 6: Verify Test - move_hunk_unrelated_different_structure_creates_conflict

**Location**: `src-tauri/src/worker/tests/mutations.rs`, lines ~609-658

This test expects a conflict when moving a hunk between unrelated commits with different file structures. **This behavior should NOT change** because we're still using 3-way merge for the destination apply.

Run this test to confirm it still passes. If it fails, investigate why.

## Testing Strategy

1. **Run existing tests first** to establish baseline:
   ```bash
   cd src-tauri && cargo test move_hunk
   ```

2. **Make implementation changes** (Steps 1-3)

3. **Run tests again** to identify failures

4. **Update failing tests** (Step 4) to match new behavior

5. **Verify other tests** (Steps 5-6) still pass

6. **Run full test suite** to ensure no regressions:
   ```bash
   cd src-tauri && cargo test
   ```

## Expected Behavior Changes

| Scenario | Old Behavior | New Behavior |
|----------|--------------|--------------|
| Move last hunk from descendant to ancestor | `PreconditionError` | Success, source abandoned |
| Partial move from descendant to ancestor | Manual tree recomputation | `rebase_descendants()` handles it |
| Move hunk to file with different structure | Creates conflict | **Same** (still creates conflict) |
| Move hunk between unrelated commits | 3-way merge | **Same** (still 3-way merge) |

## Key Insight: Why This Reduces Conflicts

The manual tree recomputation in the old descendant-to-ancestor code:
```rust
let child_new_tree = new_to_tree.clone().merge(parent_tree, from_tree).await?;
```

This performs a 3-way merge where:
- Self: `new_to_tree` (ancestor with hunk applied)
- Side 1: `parent_tree` (original parent)
- Side 2: `from_tree` (descendant with hunk + other changes)

This merge can conflict if the hunk's context differs between ancestor and descendant.

With `rebase_descendants()`, jj performs the standard rebase algorithm:
- It computes what the descendant's tree should look like given its new parent
- This uses the descendant's diff from its old parent, applied to the new parent
- Since we've set the source's tree to `remainder_tree` (hunk removed), the rebase correctly propagates only the remaining changes

## Risks and Mitigations

1. **Risk**: `rebase_descendants()` doesn't handle the case correctly
   - **Mitigation**: The `move_hunk_basic` test exercises this case; if it fails, we learn something

2. **Risk**: Abandoning source during descendant-to-ancestor causes issues
   - **Mitigation**: New test `move_hunk_descendant_abandons_source` will catch problems

3. **Risk**: Other descendants of the source get incorrect trees
   - **Mitigation**: Could add a test with multiple descendants; `rebase_descendants()` should handle this

## Implementation Checklist

- [ ] Remove `abandon_source && to_is_ancestor` precondition
- [ ] Restructure descendant-to-ancestor branch to use `rebase_descendants()`
- [ ] Update algorithm comment
- [ ] Update `move_hunk_descendant` test (rename, expect success)
- [ ] Verify `move_hunk_basic` still passes (covers partial descendant-to-ancestor)
- [ ] Verify `move_hunk_unrelated_different_structure_creates_conflict` still passes
- [ ] Run full test suite
- [ ] Manual testing with GG UI
