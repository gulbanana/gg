# GG Development Guide

## Architecture Overview

**GG is a Tauri desktop app**: Svelte/TypeScript frontend (`src/`) + Rust backend (`src-tauri/src/`). Each window has a dedicated worker thread owning a `Session` that manages jj-lib state (jj-lib is not thread-safe).

### Core Architectural Boundaries

- **`src/ipc.ts`**: Frontend IPC abstraction. Four message types:
  - `trigger()` - fire-and-forget backend actions (native UI)
  - `query()` - request data without side effects  
  - `mutate()` - structured repository mutations (goes through worker)
  - Events - server→client and client→client broadcasts via Svelte stores
  
- **`src-tauri/src/worker/mod.rs`**: Worker thread state machine. Session progresses through states:
  - `WorkerSession` - Opening/reopening workspace
  - `WorkspaceSession` - Workspace open, executes mutations
  - `QuerySession` - Paged log query in progress

- **`src-tauri/src/handler.rs`**: Error handling macros (`fatal!`, `nonfatal!`, `optional!`) for worker error propagation

### Direct Manipulation System

The UI metaphor is **drag-and-drop to edit the repository**. Core components:

- **`src/objects/Object.svelte`**: Draggable items (revisions, changes, branches)
- **`src/objects/Zone.svelte`**: Drop targets
- **`src/mutators/BinaryMutator.ts`**: Centralizes drag-drop policy. Check `canDrag()` and `canDrop()` methods to understand valid operations.

**Convention**: Actionable objects = icon + text. Greyscale = chrome/labels, colors = interactive widgets/state indicators.

## Critical Workflows

### Type Generation (CRITICAL)

After modifying Rust structs with `#[cfg_attr(feature = "ts-rs", ...)]` in `src-tauri/src/messages/`:

```bash
npm run gen  # Runs: cd src-tauri && cargo test -F ts-rs
```

This exports TypeScript types to `src/messages/`. **Frontend will break without this step.**

### Source Control
This project uses **Jujutsu (jj)** for version control instead of Git. See [jujutsu-guide.md](jujutsu-guide.md) for detailed usage instructions.

**Quick Reference:**
- View history: `jj log --no-pager`
- View diffs: `jj diff --no-pager --git --from "@-" --to "@"`
- Show commit: `jj show --no-pager --git -r "@-"`
- Commit changes: `jj commit -m "description"`

### Development Commands

```bash
npm run tauri dev              # Debug build with auto-reload
npm run test                   # Cargo tests (in src-tauri/)
npm run tauri dev -- -- -- --debug  # Pass --debug to app (yes, 3x --)
```

### Adding New Mutations

1. Define struct in `src-tauri/src/messages/mutations.rs` with `#[cfg_attr(feature = "ts-rs", derive(TS))]`
2. Implement `Mutation` trait in `src-tauri/src/worker/mutations.rs`:
   ```rust
   impl Mutation for YourMutation {
       fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
           let mut tx = ws.start_transaction()?;
           
           // Resolve commits early
           let from = ws.resolve_single_change(&self.from_id)?;
           let to = ws.resolve_single_commit(&self.to_id)?;
           
           // Check immutability before doing work
           if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
               precondition!("Revisions are immutable");
           }
           
           // Perform mutation logic...
           
           match ws.finish_transaction(tx, "description")? {
               Some(new_status) => Ok(MutationResult::Updated { new_status }),
               None => Ok(MutationResult::Unchanged),
           }
       }
   }
   ```
3. Run `npm run gen` to export types
4. Call from frontend via `mutate()` in `src/ipc.ts`

**Mutation Style Guide:**
- Start transaction early (first line after signature)
- Use consistent variable names: `from`/`to` not `source`/`target` or `from_commit`/`to_commit`
- Check immutability immediately after resolving commits
- Use `precondition!` macro for user-facing validation errors
- Keep comments minimal - code should be self-documenting
- Match style of similar mutations (e.g., `MoveChanges` for tree operations)

## Project-Specific Patterns

### Branch Object Complexity (see DESIGN.md)

Branches have multiple axes: local/remote, tracked/untracked, synced/unsynced, present/absent. **GG combines synced local+remote branches into single UI objects.**

- "Track" applies to untracked remote branches
- "Untrack" is polymorphic: all remotes (for combined) or one remote (for unsynced)
- "Delete" operates on the visible object, not combined state
- Add/green state = pushing will set remote ref; Remove/red = remote will clear ref
- Dashed border = "disconnected" (local-only or remote-only)

See `DESIGN.md` "Branch Objects" section for the full state machine.

### Configuration System

`src-tauri/src/config/gg.toml` contains defaults. Settings loaded via `jj config` (user + repo layers).

Key settings:
- `gg.queries.log-page-size` - controls paging (default 1000)
- `gg.queries.large-repo-heuristic` - disables features when repo is "too large" (default 100k)
- `gg.ui.track-recent-workspaces` - disable to prevent config file updates (default true)
- `revset-aliases.immutable_heads()` - determines editable history boundary

Access via `GGSettings` trait methods in `src-tauri/src/config/mod.rs`.

### Adding New GG Settings

1. Add default value with comment in `src-tauri/src/config/gg.toml`
2. Add trait method to `GGSettings` and implement for `UserSettings` in `src-tauri/src/config/mod.rs`
3. If frontend needs the value: add field to message struct (e.g., `RepoConfig::Workspace`), populate in worker, run `npm run gen`
4. Add tests using `settings_with_gg_defaults()` / `settings_with_overrides()` helpers in `config/mod.rs`

### Error Handling in Workers

Use macros from `handler.rs`:
- `precondition!("msg")` - return `MutationResult::PreconditionError` 
- `fatal!(result)` - panic with logging (unrecoverable)
- `nonfatal!(result)` - log and return early

### Tree Merge Semantics

jj-lib's `MergedTree::merge()` performs 3-way merges:
```rust
base_tree.merge(&side1, &side2)  // Merge side1 and side2 with base_tree as the merge base
```

Common patterns:
- **Apply changes**: `target.merge(&base, &modified)` - applies diff from base→modified to target
- **Remove changes**: `modified.merge(&base, &added)` - removes diff from base→added from modified
- **Backout**: `working.merge(&to_revert, &parent_of_to_revert)` - reverses changes

Example from `MoveChanges`:
```rust
let remainder_tree = from_tree.merge(&parent_tree, &split_tree)?;  // Remove split from from_tree
let new_to_tree = to_tree.merge(&parent_tree, &split_tree)?;      // Add split to to_tree
```

When moving changes, always consider if the destination is a descendant of the source - descendants need special handling as they inherit changes through rebasing.

### Conflict Handling

When reading file content that may have conflicts, use `jj_lib::conflicts::materialize_tree_value()` to convert conflict markers to text:

```rust
use jj_lib::conflicts::{self, ConflictMarkerStyle, MaterializedTreeValue};

match tree.path_value(path)?.into_resolved() {
    Ok(Some(TreeValue::File { id, .. })) => /* normal file */,
    Err(_) => {
        // Handle conflict by materializing to text with markers
        match conflicts::materialize_tree_value(store, path, tree.path_value(path)?).await? {
            MaterializedTreeValue::FileConflict(file) => {
                conflicts::materialize_merge_result(&file.contents, ConflictMarkerStyle::default(), &mut output)?;
            }
            _ => /* handle other cases */,
        }
    }
}
```

### Testing Mutations

Test repository (`src-tauri/resources/test-repo.zip`) contains pre-defined commits. Check which are mutable:
```bash
jj log -r 'mutable()'  # Shows commits that can be modified in tests
```

**Key test commits** (from `src-tauri/src/worker/tests/mod.rs`):
- `working_copy()` - nnloouly (empty, child of main)
- `main_bookmark()` - mnkoropy (renamed c.txt)
- `conflict_bookmark()` - nwrnuwyp (has conflict in b.txt)
- `resolve_conflict()` - rrxroxys (resolved the conflict)

Test structure:
- Use `mkid("change_id", "commit_id")` to reference specific commits
- Use `mutation.execute_unboxed(&mut ws)?` to run mutations
- Use `assert_matches!(result, MutationResult::Updated { .. })` to verify success
- Immutable commits will fail with `PreconditionError`

### JJ Version Coupling

jj-lib and jj-cli dependencies are pinned to specific versions (currently 0.29). Changes must be compatible with the declared version. The app embeds jj functionality - users don't need jj CLI installed.

## Key Files to Reference

- `DESIGN.md` - Core metaphors, architectural decisions, branch state machine
- `src/mutators/BinaryMutator.ts` - All drag-drop operation policies
- `src-tauri/src/worker/mutations.rs` - All mutation implementations (21+ examples)
- `src-tauri/src/config/gg.toml` - Default configuration with inline docs
- `src/stores.ts` - Global Svelte stores for cross-component state

## Svelte Patterns

### Virtualized Lists (GraphLog.svelte)

The log pane uses virtualization - a fixed pool of DOM slots that get reused as you scroll. Key by **slot index**, not by content:

```svelte
<!-- CORRECT: Key by slot position for virtualization -->
{#each visibleSlice.rows as row, i (i)}
    <RevisionObject header={row.revision} />
{/each}

<!-- WRONG: Content-based keys fight virtualization -->
{#each visibleSlice.rows as row, i}
    {#key row?.revision.id.commit.hex ?? i}
        <RevisionObject header={row.revision} />
    {/key}
{/each}
```

Content-based keys cause components to be destroyed/recreated when slots scroll to show different data, defeating the purpose of virtualization. Slot-index keys let Svelte efficiently update props on the same component instances.

### Reactive Derived Values

When component props are used to compute derived values, use `$:` reactive statements - not one-time initialization in the script body:

```svelte
<!-- WRONG: Computed once at mount, won't update when ref changes -->
<script>
    export let ref;
    let label = ref.branch_name;  // stale after prop update!
</script>

<!-- CORRECT: Recomputes when ref changes -->
<script>
    export let ref;
    $: label = ref.branch_name;
</script>
```

This is especially important for components rendered in virtualized lists, where the same component instance receives different props as slots are reused.
