# GG Development Guide

## Architecture Overview

**GG is a desktop app** with two components: Svelte/TypeScript frontend (`app/`) + Rust backend (`src/`). Each window has a dedicated worker thread owning a `Session` that manages jj-lib state (jj-lib is not thread-safe).

### Launch Modes

- **GUI mode** (`src/gui/mod.rs`): Tauri desktop app with native windowing, menus, and IPC. Supports multiple windows (each with its own worker thread).
- **Web mode** (`src/web/mod.rs`): Axum HTTP server that serves the frontend in a browser. Single worker, single session.

Both modes share the same frontend code. The frontend detects its runtime via `isTauri()` in `app/ipc.ts` and uses the appropriate transport layer (Tauri IPC vs HTTP).

Mode selection: CLI subcommands (`gg gui`, `gg web`) or `gg.default-mode` config setting.

See `.github/specs/gui-mode.md` and `.github/specs/web-mode.md` for detailed technical specifications.

### Core Architectural Boundaries

- **`app/ipc.ts`**: Frontend IPC abstraction with dual transport support. Key exports:
  - `isTauri()` - runtime detection: checks for `__TAURI_INTERNALS__` in window
  - `trigger()` - fire-and-forget backend actions (uses `sendBeacon` in web mode)
  - `query()` - request data without side effects  
  - `mutate()` - structured repository mutations (goes through worker)
  - `event()` - creates event-backed Svelte stores (Tauri events in GUI, local-only in web)
  
  In GUI mode, calls route through Tauri's `invoke()`. In web mode, calls use `fetch()` to `/api/{query|trigger|mutate}/{command}` endpoints.
  
- **`src/worker/mod.rs`**: Worker thread state machine. Session progresses through states:
  - `WorkerSession` - Opening/reopening workspace
  - `WorkspaceSession` - Workspace open, executes mutations
  - `QuerySession` - Paged log query in progress

- **`src/main.rs`**: CLI parsing and `RunOptions` struct shared by both launch modes

- **`src/gui/handler.rs`**: Error handling macros (`fatal!`, `nonfatal!`, `optional!`) for GUI error propagation

- **Multi-window support (GUI only)**: `AppState` contains a `HashMap<String, WindowState>` keyed by window label. Each `WindowState` owns its worker thread and channel. Window labels are hashed from workspace paths.

### Direct Manipulation System

The UI metaphor is **drag-and-drop to edit the repository**. Core components:

- **`app/objects/Object.svelte`**: Draggable items (revisions, changes, branches)
- **`app/objects/Zone.svelte`**: Drop targets
- **`app/mutators/BinaryMutator.ts`**: Centralizes drag-drop policy. Check `canDrag()` and `canDrop()` methods to understand valid operations.

**Convention**: Actionable objects = icon + text. Greyscale = chrome/labels, colors = interactive widgets/state indicators.

## Critical Workflows

### Type Generation (CRITICAL)

After modifying Rust structs with `#[cfg_attr(feature = "ts-rs", ...)]` in `src/messages/`:

```bash
cargo gen  # Runs: cargo test -F ts-rs
```

This exports TypeScript types to `app/messages/`. **Frontend will break without this step.**

### Source Control
This project uses **Jujutsu (jj)** for version control instead of Git. See [jujutsu-guide.md](jujutsu-guide.md) for detailed usage instructions.

**Quick Reference:**
- View history: `jj log --no-pager`
- View diffs: `jj diff --no-pager --git --from "@-" --to "@"`
- Show commit: `jj show --no-pager --git -r "@-"`
- Commit changes: `jj commit -m "description"`

### Development Commands

```bash
cargo tauri dev              # Debug build with auto-reload
cargo test                   # Cargo tests
cargo tauri dev -- -- --debug  # Pass --debug to app (yes, 2x --)
cargo run                    # Spawns GUI in background (uses prebuilt assets)
cargo run -- web             # Launch in web mode (opens browser)
```

### Adding New Mutations

1. Define struct in `src/messages/mutations.rs` with `#[cfg_attr(feature = "ts-rs", derive(TS))]`
2. Implement `Mutation` trait in `src/worker/mutations.rs`:
   ```rust
   #[async_trait::async_trait(?Send)]
   impl Mutation for YourMutation {
       async fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<MutationResult> {
           let mut tx = ws.start_transaction().await?;
           
           // Resolve commits early
           let from = ws.resolve_single_change(&self.from_id)?;
           let to = ws.resolve_single_commit(&self.to_id)?;
           
           // Check immutability before doing work
           if ws.check_immutable(vec![from.id().clone(), to.id().clone()])? {
               precondition!("Revisions are immutable");
           }
           
           // Perform mutation logic...
           
           match ws.finish_transaction(tx, "description")? {
               Some(new_status) => Ok(MutationResult::Updated { new_status, new_selection: None }),
               None => Ok(MutationResult::Unchanged),
           }
       }
   }
   ```
3. Run `cargo gen` to export types
4. Call from frontend via `mutate()` in `app/ipc.ts`

**Mutation Style Guide:**
- Start transaction early (first line after signature)
- Use consistent variable names: `from`/`to` not `source`/`target` or `from_commit`/`to_commit`
- Check immutability immediately after resolving commits
- Use `precondition!` macro for user-facing validation errors
- Match style of similar mutations (e.g., `MoveChanges` for tree operations)

### Comment Style

Use lowercase, and prefer minimal comments. Don't describe *what* the code does - that's what the code is for. Comments should explain *why* when the reason isn't obvious, or warn about non-obvious gotchas.

```rust
// BAD: Describes what the code does, uses uppercase
// Read recent workspaces for initial menu build
let recent_workspaces = settings.ui_recent_workspaces();
build_menu(recent_workspaces);

// BAD: Obvious from the code
// check if the revision is immutable
if ws.check_immutable(vec![commit.id().clone()])? { ... }

// GOOD: Explains why (non-obvious reason)
// must clone before the builder chain - settings moves into .manage()
let recent_workspaces = settings.ui_recent_workspaces();

// GOOD: Warns about a gotcha
// jj-lib is not thread-safe; each window needs its own worker
let window_worker = thread::spawn(move || { ... });
```

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

`src/config/gg.toml` contains defaults. Settings loaded via `jj config` (user + repo layers).

Key settings:
- `gg.default-mode` - launch mode when no subcommand given: "gui" (default) or "web"
- `gg.queries.log-page-size` - controls paging (default 1000)
- `gg.queries.large-repo-heuristic` - disables features when repo is "too large" (default 100k)
- `gg.ui.track-recent-workspaces` - disable to prevent config file updates (default true)
- `revset-aliases.immutable_heads()` - determines editable history boundary

Access via `GGSettings` trait methods in `src/config/mod.rs`.

### Adding New GG Settings

1. Add default value with comment in `src/config/gg.toml`
2. Add trait method to `GGSettings` and implement for `UserSettings` in `src/config/mod.rs`
3. If frontend needs the value: add field to message struct (e.g., `RepoConfig::Workspace`), populate in worker, run `cargo gen`
4. Add tests using `settings_with_gg_defaults()` / `settings_with_overrides()` helpers in `config/tests.rs`

### Error Handling (GUI Mode)

Use macros from `src/gui/handler.rs` for GUI-specific error handling:
- `fatal!(result)` - panic with logging (unrecoverable)
- `nonfatal!(result)` - log and return early
- `optional!(result)` - silently ignore errors (for optional operations like focus)

These macros are GUI-only. In web mode, all operations are strictly request-response, so errors are always returned directly to the frontend via HTTP responses.

In mutations, use the `precondition!` macro (defined in `mutations.rs`) to return `MutationResult::PreconditionError` for user-facing validation errors.

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

Test repository (`res/test-repo.zip`) contains pre-defined commits. Check which are mutable:
```bash
jj log -r 'mutable()'  # Shows commits that can be modified in tests
```

**Key test commits** (from `src/worker/tests/mod.rs`):
- `working_copy()` - mntpnnrk (empty, child of main)
- `main_bookmark()` - mnkoropy (renamed c.txt)
- `conflict_bookmark()` - nwrnuwyp (has conflict in b.txt)
- `resolve_conflict()` - rrxroxys (resolved the conflict)

Test structure:
- Use `mkid("change_id", "commit_id")` to reference specific commits
- Use `mutation.execute_unboxed(&mut ws).await?` to run mutations (async)
- Use `assert_matches!(result, MutationResult::Updated { .. })` to verify success
- Immutable commits will fail with `PreconditionError`

### JJ Version Coupling

jj-lib and jj-cli dependencies are pinned to specific versions (see `Cargo.toml`). Changes must be compatible with the declared version. The app embeds jj functionality - users don't need jj CLI installed.

## Key Files to Reference

- `DESIGN.md` - Core metaphors, architectural decisions, branch state machine
- `.github/specs/gui-mode.md` - GUI mode technical specification
- `.github/specs/web-mode.md` - Web mode technical specification
- `app/mutators/BinaryMutator.ts` - All drag-drop operation policies
- `app/ipc.ts` - IPC abstraction with runtime detection and dual transport
- `src/main.rs` - CLI parsing, `RunOptions` struct, mode dispatch
- `src/worker/mutations.rs` - All mutation implementations
- `src/config/gg.toml` - Default configuration with inline docs
- `src/gui/mod.rs` - GUI mode: Tauri windowing, multi-window state, IPC handlers
- `src/gui/menu.rs` - GUI mode native menu bar and context menu definitions
- `src/web/mod.rs` - Web mode: Axum server, HTTP API routes
- `src/web/queries.rs` - Web mode query handlers
- `src/web/triggers.rs` - Web mode trigger handlers
- `app/controls/ContextMenu.svelte` - Web mode HTML context menu
- `app/stores.ts` - Global Svelte stores for cross-component state

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
