# Multiselection Specification

## Overview

Multiselection allows users to select a contiguous, linear sequence of commits in the log pane rather than just a single commit. This enables both viewing combined changes across the range and performing bulk operations on multiple revisions at once.

## Core Concepts

### RevSet (Selection Range)

A selection is represented by a `RevSet`:

```rust
pub struct RevSet {
    pub from: RevId,  // Topologically earlier (ancestor)
    pub to: RevId,    // Topologically later (descendant)
}
```

- **Single selection**: `from == to`
- **Range selection**: `from != to`, representing `from::to` (from is always the ancestor)

The frontend guarantees topological ordering: `from` is always before-or-equal-to `to` in the DAG. The UI tracks a separate "anchor" (the first-clicked revision) to preserve expected shift-click behavior, but this is internal to the log pane component.

### Linearity Constraint

**Multiselection is restricted to linear commit sequences.** A sequence is linear if:

1. Each commit has exactly one parent (no merge commits in range)
2. Each commit is a direct parent of the next (no elided commits between them)
3. The commits form a contiguous path in the DAG

This constraint exists because:
- Combined diffs only make sense for linear history
- Many bulk operations (squash, fold) require linear sequences
- The mental model is simpler: a range is like selecting lines in a file

## User Interaction

### Selection Methods

| Action | Behavior |
|--------|----------|
| Click revision | Select single revision (clears range) |
| Ctrl+Click | Not currently supported (reserved for future disjoint selection) |
| Shift+Click | Extend selection from anchor to clicked revision (limited by linearity) |
| Arrow Up/Down | Move selection (collapses range to single) |
| Shift+Arrow Up/Down | Extend/contract `to` endpoint by one |
| Page Up/Down | Move selection by page (collapses range) |
| Home/End | Move to first/last revision (collapses range) |

### Linearity Limiting

When extending selection (Shift+Click or Shift+Arrow):
- The UI finds the farthest commit toward the target that maintains linearity
- If target is reachable linearly, selection extends to target
- If blocked by merge/branch point, selection stops at the last linear commit
- No visual feedback when limited (selection just doesn't extend further)

**Design note:** The lack of feedback when extension is blocked is intentional. If this proves confusing in practice, we may revisit by making disjoint graph sections more visually distinct.

## Display (Right Pane)

### Single Selection

Same as current behavior:
- Shows change ID and commit ID
- Full author/timestamp
- Editable description
- Individual changes and hunks
- All actions enabled (Edit, New, Describe, Squash, Restore)

### Range Selection

Header shows: `<first-change-id> :: <last-change-id> (N revisions)`

| Element | Behavior |
|---------|----------|
| Description | Read-only combined view: all descriptions shown with dashed dividers between them |
| Author | Combined view: unique authors (deduplicated by email) with timestamp range (earliest to latest) |
| Parents | Shows parents of oldest commit in range |
| Changes | Combined diff: oldest parent → newest commit |
| Conflicts | Combined view: merged with Changes and sorted by path (not separately displayed) |

**Enabled Actions:**
- New child – creates merge commit with all selected revisions as parents
- New parent – inserts new empty commit before oldest revision (all selected rebased onto it)
- Squash – moves combined changes to oldest commit's parent, abandons all in range
- Abandon – abandons all selected revisions
- Duplicate – duplicates the entire linear sequence
- Backout – creates backout commits for all selected (applies to working copy)

**Disabled Actions:**
- Edit (makes working copy) – ambiguous which commit
- Describe – no bulk describe support yet
- Restore – semantics unclear for ranges (what would "restore from parent" mean for multiple commits?)

## Backend Query

### `query_revisions` (plural)

```typescript
query<RevsResult>("query_revisions", { set: RevSet })
```

**Algorithm:**
1. Collect commits in topological order (descendants first)
2. Compute combined tree: diff from oldest parent to newest commit
3. Return all headers, combined changes, and combined conflicts

**Response:**
```rust
pub enum RevsResult {
    NotFound { set: RevSet },
    Detail {
        set: RevSet,              // The queried revision set
        headers: Vec<RevHeader>,  // All revisions, descendants first
        parents: Vec<RevHeader>,  // Parents of oldest revision
        changes: Vec<RevChange>,  // Combined diff
        conflicts: Vec<RevConflict>,
    },
}
```

## Bulk Operations

These menu commands accept `RevSet` and work on multiselection:

| Operation | Behavior |
|-----------|----------|
| Abandon | Abandon all selected revisions |
| Duplicate | Duplicate the entire linear sequence (preserves internal parent relationships) |
| Backout | Create backout commits for all selected (applies to working copy) |
| Squash | Squash entire range into parent (fold-like) |
| New child | Create merge between the range's commits |
| New parent | Create parent of the range's start |

## Drag-and-Drop

When dragging a multiselection:
- ✓ Visual feedback: all selected revisions highlight during drag (via `isInSource` in `Object.svelte`)
- ✓ Drop to Repository: abandons all selected revisions
- ✓ Drop to Revision: moves entire range to new parent (via `MoveRevisions` mutation)
- ✓ Drop to Parent: inserts entire range between two commits (via `InsertRevisions` mutation)
- ✓ Drop to Merge: adds range as additional parents (via `AdoptRevision` mutation with combined parent list)

## Right Pane Design

The current `RevisionPane.svelte` design for multi-revision selection is relatively complete. Some features work:
- Range header showing change ID range and count
- Multiple author display with date ranges
- Combined description display
- Combined changes display (diff from oldest parent to newest commit)
- Abandon, Duplicate, Backout actions (enabled via context menu)
- Context menu and drag-drop targets for files

Some features are disabled pending design:
- Edit/Describe/Restore actions (probably doesn't make sense)
- Context menu and drag-drop targets for hunks (non-trivial implementation!)

## Future Work

### Hunk Change Operations (file and hunk)

It should be possible to implement multiselect versions of MoveChanges and MoveHunk - but not easy. They'll need to modify subsets of the trees of multiple revisions, corresponding to an operation on the *combined* diff.

### Disjoint (Non-Contiguous) Selection

Selecting disconnected revisions (e.g., Ctrl+Click to add) is a possible future extension. The current linear-only constraint exists because:
- Simpler mental model for initial implementation
- Combined diffs only make sense for contiguous commits
- Many bulk operations (squash, fold) require linearity

A concrete design for disjoint selection exists but is deferred. If implemented, it would likely:
- Use Ctrl+Click to toggle individual revisions
- Represent selection as a set of revision IDs rather than a range
- Only enable operations that work on independent revisions (e.g., abandon, duplicate)

## Implementation Notes

### Graph Line Inspection

Linearity checking uses the graph lines from `EnhancedRow`:

```typescript
function isDirectParent(childRow: EnhancedRow, parentRow: EnhancedRow): boolean {
    // 1. Check parent_ids contains parent's commit
    // 2. Verify graph line connects them directly (not indirect, not ToMissing)
}
```

This relies on the log pane having rendered the graph lines, so linearity is checked against the visible graph, not the full DAG.

### Performance Considerations

- `query_revisions` iterates the revset twice (existence check, then collect)
- Large selections could have many commits; consider pagination or limits
- Immutability check optimization: if oldest commit is immutable, all are

### Store Integration

Two stores manage selection state:

```typescript
// The selection range (what the user selected)
revisionSelectEvent: Writable<RevSet | undefined>

// The resolved headers from query_revisions (for enablement and mutations)
selectionHeaders: Writable<RevHeader[]>
```

**Data flow:**
1. User selects revisions → `revisionSelectEvent` set to `RevSet`
2. `App.svelte` calls `query_revisions` with the `RevSet`
3. On success, `selectionHeaders` is populated from `RevsResult.Detail.headers`
4. Context menus and mutations read from `selectionHeaders`

When a mutation returns `new_selection`, it's wrapped as single-element range:
```typescript
revisionSelectEvent.set({ from: value.new_selection.id, to: value.new_selection.id });
```

### Context Menu Architecture

Context menus work differently in GUI mode vs web mode:

**GUI mode (Tauri):**
1. Right-click triggers `forward_context_menu` with an `Operand`
2. Backend receives `Operand::Revision` or `Operand::Revisions` with full headers
3. `compute_revision_enablement()` in `menu.rs` determines enabled items
4. Native context menu shown with correct enablement

**Web mode:**
1. Right-click sets `hasMenu` store with coordinates
2. `ContextMenu.svelte` renders with the operand
3. Enablement computed client-side using `selectionHeaders` store
4. `RevisionMutator` also uses `selectionHeaders` for action dispatch

**Transitional state:** The `Operand::Revisions` variant still carries headers for GUI mode compatibility, but `ContextMenu.svelte` ignores `operand.headers` and reads from `selectionHeaders` instead. This ensures consistency between the displayed enablement and the actual mutation target.
