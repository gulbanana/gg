# Project Overview

GG is a GUI for Jujutsu (jj) version control. It's a Tauri desktop app with Svelte/TypeScript frontend (`app/`) and Rust backend (`src/`). Two launch modes: native GUI (`gg gui`) and browser-based web mode (`gg web`).

## Development Commands

```bash
npm install && npm run build   # Initial frontend build (required first time)
cargo tauri dev                # Debug build with auto-reload
cargo tauri dev -- -- --debug  # Pass --debug flag to app (yes, 2x --)
cargo test                     # Run Rust tests
cargo gen                      # CRITICAL: Regenerate TypeScript types after modifying Rust structs
cargo run                      # Launch GUI (uses prebuilt assets)
cargo run -- web               # Launch in web mode (opens browser)
```

**Before committing:** Run `cargo clippy` and `cargo fmt` to ensure code quality and consistent formatting.

## Architecture

Each window has a dedicated worker thread owning a `Session` (jj-lib is not thread-safe). The session progresses through states: `WorkerSession` → `WorkspaceSession` → `QuerySession`.

### Key Boundaries

- **`app/ipc.ts`**: Frontend transport abstraction. `isTauri()` detects runtime. Exports `query()`, `mutate()`, `trigger()`, `event()`.
- **`src/worker/mod.rs`**: Worker thread state machine processing `SessionEvent`s.
- **`src/gui/mod.rs`**: Tauri setup, multi-window state (`HashMap<String, WindowState>`), IPC handlers.
- **`src/web/mod.rs`**: Axum HTTP server with `/api/{query|trigger|mutate}/{command}` endpoints.

### IPC Categories

1. **Triggers**: Fire-and-forget backend actions (native UI operations)
2. **Queries**: Request data without side effects
3. **Mutations**: Structured repository modifications
4. **Events**: Push updates to frontend (Tauri events in GUI, local-only in web)

## Type Generation (CRITICAL)

After modifying Rust structs with `#[cfg_attr(feature = "ts-rs", ...)]` in `src/messages/`:
```bash
cargo gen  # Runs: cargo test -F ts-rs
```
This exports TypeScript types to `app/messages/`. **Frontend breaks without this step.**

## Adding New Mutations

1. Define struct in `src/messages/mutations.rs` with `#[cfg_attr(feature = "ts-rs", derive(TS))]`
2. Implement `Mutation` trait in `src/worker/mutations.rs`:
   - Start transaction first
   - Use `from`/`to` variable names (not `source`/`target`)
   - Check immutability immediately after resolving commits
   - Use `precondition!` macro for user-facing validation errors
3. Run `cargo gen`
4. Call from frontend via `mutate()` in `app/ipc.ts`

## Testing Mutations

Test repository (`res/test-repo.zip`) contains pre-defined commits. Key test commits (from `src/worker/tests/mod.rs`):
- `working_copy()` - mntpnnrk (empty, child of main)
- `main_bookmark()` - mnkoropy (renamed c.txt)
- `conflict_bookmark()` - nwrnuwyp (has conflict in b.txt)
- `resolve_conflict()` - rrxroxys (resolved the conflict)

Test patterns:
- `mkid("change_id", "commit_id")` to reference commits
- `mutation.execute_unboxed(&mut ws).await?` to run mutations
- `assert_matches!(result, MutationResult::Updated { .. })` for success
- Immutable commits fail with `PreconditionError`

## JJ Version Coupling

jj-lib and jj-cli dependencies are pinned to specific versions (see `Cargo.toml`). The app embeds jj functionality - users don't need jj CLI installed.

## Key Patterns

### Tree Merge Semantics

jj-lib's `MergedTree::merge()` performs 3-way merges:
- **Apply changes**: `target.merge(&base, &modified)` - applies diff base→modified to target
- **Remove changes**: `modified.merge(&base, &added)` - removes diff base→added from modified
- **Backout**: `working.merge(&to_revert, &parent_of_to_revert)` - reverses changes

### Direct Manipulation (Drag & Drop)

UI metaphor: drag-and-drop to edit repository. Policy centralized in `app/mutators/BinaryMutator.ts` - check `canDrag()` and `canDrop()` for valid operations.

### Svelte Virtualization

The log pane uses virtualization. Key by slot index, not content:
```svelte
<!-- CORRECT -->
{#each visibleSlice.rows as row, i (i)}

<!-- WRONG - fights virtualization -->
{#key row?.revision.id.commit.hex ?? i}
```

### Reactive Props

Use `$:` for derived values from props (critical in virtualized lists):
```svelte
$: label = ref.bookmark_name;  // CORRECT: reactive
let label = ref.bookmark_name; // WRONG: computed once at mount
```

### Error Handling (GUI Mode)

Use macros from `src/gui/handler.rs`:
- `fatal!(result)` - panic with logging
- `nonfatal!(result)` - log and return early
- `optional!(result)` - silently ignore (for optional operations)

## Configuration

Settings loaded via `jj config`. Key GG-specific settings in `src/config/gg.toml`:
- `gg.default-mode` - "gui" or "web"
- `gg.queries.log-page-size` - paging size (default 1000)
- `gg.queries.large-repo-heuristic` - disables features when repo too large (default 100k)

## Comment Style

Lowercase, minimal. Don't describe *what* code does. Explain *why* when non-obvious, or warn about gotchas:
```rust
// GOOD: explains why
// must clone before the builder chain - settings moves into .manage()

// BAD: describes what
// Read recent workspaces for initial menu build
```

## Import Style

Prefer `use` for structs/types, but qualify function calls using the module they're in:
```rust
use anyhow::{Context, Error, Result};
use jj_cli::git_util;

// GOOD: struct via use, function qualified by containing module
let err = Error::new(e.error);
let options = git_util::load_git_import_options(&ui, &settings);

// BAD: qualifying struct, qualifying function by crate
let err = anyhow::Error::new(e.error);
let options = jj_cli::git_util::load_git_import_options(&ui, &settings);
```

## TypeScript Style

Use `let` instead of `const` for variable declarations.

## Key Files Reference

- `DESIGN.md` - Core metaphors, bookmark state machine
- `app/mutators/BinaryMutator.ts` - All drag-drop policies
- `app/ipc.ts` - IPC abstraction with runtime detection
- `src/worker/mutations.rs` - All mutation implementations
- `src/config/gg.toml` - Default configuration with inline docs
