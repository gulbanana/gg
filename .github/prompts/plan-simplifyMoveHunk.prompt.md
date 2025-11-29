# Implementation Plan: Simplify MoveHunk to Use 3-Way Merge Everywhere

## Goal

Remove the special-case textual patching for unrelated commits in `MoveHunk` and use consistent 3-way merge semantics for all cases. This implements "split-then-squash" behavior at the tree level, making the code simpler and more predictable while surfacing real conflicts rather than using heuristic patching.

## Background

### Current Algorithm Problems

The current `MoveHunk` implementation in `src-tauri/src/worker/mutations.rs` (lines 896-1102) has two fundamentally different code paths:

1. **Related commits** (ancestor/descendant): Uses 3-way merge
2. **Unrelated commits**: Uses textual hunk application with fuzzy context matching

The textual patching approach is unreliable because it applies hunks to trees they weren't computed against. The `find_hunk_position` function searches for matching context lines anywhere in the file, which can fail or produce wrong results.

### Why 3-Way Merge Is Better

When building `hunk_tree`, we apply the hunk to `parent_tree` — the tree it was originally computed against. This is tautological and cannot fail. Then 3-way merges handle the actual movement:

- **Remove from source**: `from_tree.merge(hunk_tree, parent_tree)` — backs out the hunk
- **Apply to destination**: `to_tree.merge(parent_tree, hunk_tree)` — applies the hunk

Conflicts from these merges represent real semantic ambiguity, not heuristic failures.

### Equivalence to Split-then-Squash

This approach is equivalent to `jj split` followed by `jj squash`, but operates purely on trees without creating intermediate revisions. The `hunk_tree` is the tree content that a "split child revision" would have.

## Files to Modify

- `src-tauri/src/worker/mutations.rs` — main implementation changes
- `src-tauri/src/worker/tests/mutations.rs` — test updates

## Step 1: Remove the `is_related` Branching

### Current Code Structure (lines 947-1003)

```rust
// check if commits are related, which disallows conflict-free copies
let from_is_ancestor = tx.repo().index().is_ancestor(from.id(), to.id())?;
let to_is_ancestor = tx.repo().index().is_ancestor(to.id(), from.id())?;
let is_related = from_is_ancestor || to_is_ancestor;

let (remainder_tree, new_to_tree) = if is_related {
    // for related commits, we need 3-way merge (may create conflicts)
    let remainder = from_tree
        .clone()
        .merge(hunk_tree.clone(), parent_tree.clone())
        .await?;
    let to_tree = to.tree()?;
    let new_to = to_tree
        .merge(parent_tree.clone(), hunk_tree.clone())
        .await?;
    (remainder, new_to)
} else {
    // for unrelated commits, apply hunk directly to avoid conflicts
    // ... ~45 lines of textual patching code including reverse hunk creation ...
};
```

### Changes Required

1. Keep the `from_is_ancestor` and `to_is_ancestor` checks — they're still needed for the post-merge logic
2. Remove the `is_related` variable entirely
3. Remove the `if is_related { ... } else { ... }` branching
4. Keep only the 3-way merge code path, but move `to_tree` lookup before the merges:

```rust
let from_is_ancestor = tx.repo().index().is_ancestor(from.id(), to.id())?;
let to_is_ancestor = tx.repo().index().is_ancestor(to.id(), from.id())?;

let to_tree = to.tree()?;
let remainder_tree = from_tree
    .clone()
    .merge(hunk_tree.clone(), parent_tree.clone())
    .await?;
let new_to_tree = to_tree
    .merge(parent_tree.clone(), hunk_tree.clone())
    .await?;
```

### What to Delete

The entire `else` block (lines ~957-1003) which contains:
- `to_tree` lookup and content reading
- `apply_hunk` call for destination
- `update_tree_entry` for destination
- Reverse hunk construction (the `filter_map` that swaps `+`/`-` prefixes)
- `from_content` reading
- `apply_hunk` call for source with reverse hunk
- `update_tree_entry` for source remainder

## Step 2: Simplify Hunk Application Helpers

After removing the unrelated-commits branch, hunks are only ever applied to their original base tree (`parent_tree`). This means we have exact line numbers and don't need fuzzy context matching.

### Delete `find_hunk_position` (lines 1657-1693)

This function searches for matching context lines anywhere in a file. It was needed for the unreliable case of applying hunks to arbitrary trees.

```rust
fn find_hunk_position(
    base_lines: &[&str],
    hunk: &crate::messages::ChangeHunk,
    suggested_start: usize,
) -> Result<usize> {
    // ... fuzzy context matching logic ...
}
```

**Delete entirely.**

### Simplify `apply_hunk` (lines 1697-1755)

Currently `apply_hunk` calls `find_hunk_position` to search for where to apply. Since we now only apply hunks to their original base tree, we can use the exact line numbers from `hunk.location.from_file.start`.

**Change from:**
```rust
let hunk_start_line_0_based = hunk.location.from_file.start.saturating_sub(1);
let actual_start = find_hunk_position(&base_lines, hunk, hunk_start_line_0_based)?;
```

**Change to:**
```rust
let actual_start = hunk.location.from_file.start.saturating_sub(1);
// Context verification happens inline during hunk application (existing code)
```

The existing context line verification in the main loop (the `if base_lines[base_line_idx].trim_end() == hunk_content_part.trim_end()` check) serves as an assertion that the hunk matches, failing fast with a clear error if something is wrong.

**Benefits:**
1. More efficient (no searching)
2. Fails fast with clear error if hunk doesn't match (rather than finding false match elsewhere)
3. Makes intent clearer — we're applying at a known location, not searching

## Step 3: Update Tests

The tests in `src-tauri/src/worker/tests/mutations.rs` need to be updated to reflect the new merge-based behavior.

### Existing Tests

| Test | Lines | Scenario | Expected Changes |
|------|-------|----------|------------------|
| `move_hunk_basic` | 349-399 | Move from `hunk_source` to `working_copy` | May need to verify merge result vs textual patch result |
| `move_hunk_message` | 404-457 | Abandoning source combines descriptions | Should work the same |
| `move_hunk_invalid` | 462-502 | Invalid hunk content | Should still fail, possibly with different error |
| `move_hunk_descendant` | 507-548 | Child→Ancestor where child becomes empty | Should still return `PreconditionError` |
| `move_hunk_unrelated` | 553-612 | Sibling commits (unrelated) | **Most likely to change** — was using textual patch, now uses 3-way merge |
| `move_hunk_partial` | 875-931 | Move one hunk, keep another | May produce different results |

### Test Update Strategy

1. **Run tests first** to see which actually fail
2. For failing tests, examine whether the new behavior is correct (real conflicts) or indicates a bug
3. Update assertions to match the new (correct) merge-based behavior
4. Consider adding comments explaining that the test verifies 3-way merge semantics

### Key Test: `move_hunk_unrelated`

This test moves a hunk between sibling commits (`hunk_child_single` → `hunk_sibling`). Currently it uses textual patching. With 3-way merge:

- Source file in `hunk_child_single`: `line1\nmodified2\nline3\nline4\nline5\n`
- Source file in `hunk_sibling`: `line1\nline2\nline3\nline4\nline5\nnew6\nnew7\nnew8\n`
- The hunk represents `line2` → `modified2`
- 3-way merge should cleanly apply this since the regions don't overlap

This test should still pass, but verify the exact file contents match expectations.

## Step 4: Assess Post-Merge Logic Restructuring

The current post-merge logic (lines 1005-1068) has three branches:

```rust
if abandon_source {
    // Source becomes empty
    if to_is_ancestor { precondition!(...); }
    tx.repo_mut().record_abandoned_commit(&from);
    // rewrite destination
} else if to_is_ancestor {
    // Descendant-to-ancestor special case
    // rewrite destination first, then recompute source tree, then rebase source
} else {
    // General case: unrelated or ancestor-to-descendant
    // rewrite source
    if from_is_ancestor { rebase descendants and update `to` }
    // rewrite destination
    tx.repo_mut().rebase_descendants()?;
}
```

### Compare to `MoveChanges` Structure

`MoveChanges` (lines 425-498) has a simpler flow:

```rust
// 1. Abandon or rewrite source
if abandon_source {
    tx.repo_mut().record_abandoned_commit(&from);
} else {
    tx.repo_mut().rewrite_commit(&from).set_tree_id(...).write()?;
}

// 2. Rebase descendants (handles from_is_ancestor case)
if tx.repo().index().is_ancestor(from.id(), to.id())? {
    // rebase and update `to` reference
}

// 3. Apply to destination (always)
tx.repo_mut().rewrite_commit(&to).set_tree_id(...).write()?;
```

### Assessment Questions

1. **Can `MoveHunk` use the same structure?** The `to_is_ancestor` case seems fundamentally different — it requires recomputing the source tree after the destination is rewritten. This might not simplify.

2. **Is the `to_is_ancestor` + `abandon_source` precondition still necessary?** It blocks "moving a hunk from a commit that becomes empty to an ancestor". With proper 3-way merge, would this just work?

3. **Does the final `rebase_descendants()` in the general case duplicate work?** The `from_is_ancestor` branch already calls `rebase_descendants_with_options`. Is the final call necessary?

4. **Is more test coverage necessary to verify edge cases?** If we modified the structure, what are the areas of risk?

### Assessment Results

If feasible, create a new commit, then update MoveHunk to work more like MoveChanges Ensure that it passes all the same tests - unlike earlier steps, this one should not change behaviour.

## Implementation Checklist

- [ ] Remove `is_related` variable and the `else` branch (Step 1)
- [ ] Move `to_tree` lookup before the merges (Step 1)
- [ ] Delete `find_hunk_position` function (Step 2)
- [ ] Simplify `apply_hunk` to use exact line numbers (Step 2)
- [ ] Run existing tests to identify failures (Step 3)
- [ ] Update test assertions for new merge-based behavior (Step 3)
- [ ] Document the "split-then-squash" equivalence in a code comment (optional)
- [ ] Assess post-merge restructuring feasibility (Step 4, document findings)

## Expected Outcomes

1. **Simpler code**: ~45 lines of textual patching removed, `find_hunk_position` deleted
2. **Consistent semantics**: All cases use 3-way merge, matching `jj squash` behavior
3. **Better conflict handling**: Real conflicts surfaced instead of heuristic failures
4. **Maintainability**: One algorithm to understand and test, not two
5. **Clearer intent**: Hunk application uses exact positions, not fuzzy search
