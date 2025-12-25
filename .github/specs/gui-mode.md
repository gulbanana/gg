# GUI Mode Technical Specification

GUI mode is GG's primary desktop experience, built on Tauri with native windowing, menus, and IPC. This document describes the architecture and key implementation details.

## Overview

GG supports two launch modes:
- **GUI mode**: Tauri desktop app with native windowing and IPC
- **Web mode**: Axum server serving the frontend in a browser

Both modes share the same frontend code. The frontend detects its runtime environment and uses the appropriate transport layer.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Tauri WebView Window                        │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    Svelte Frontend                          ││
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐      ││
│  │  │  stores.ts  │  │  ipc.ts     │  │  Components     │      ││
│  │  │  (state)    │  │  (transport)│  │  (UI)           │      ││
│  │  └─────────────┘  └──────┬──────┘  └─────────────────┘      ││
│  └──────────────────────────┼──────────────────────────────────┘│
│                             │ Tauri IPC                         │
├─────────────────────────────┼───────────────────────────────────┤
│                        Rust Backend                             │
│  ┌──────────────────────────┴──────────────────────────────────┐│
│  │                   #[tauri::command]                         ││
│  │                    Command Handlers                         ││
│  └──────────────────────────┬──────────────────────────────────┘│
│                             │ mpsc channel                      │
│  ┌──────────────────────────┴──────────────────────────────────┐│
│  │                    Worker Thread                            ││
│  │              (one per window, owns Session)                 ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Key Files

| File | Purpose |
|------|---------|
| [src/gui/mod.rs](../../src/gui/mod.rs) | Tauri app setup, window management, IPC command handlers |
| [src/gui/menu.rs](../../src/gui/menu.rs) | Native menu bar and context menu definitions |
| [src/gui/handler.rs](../../src/gui/handler.rs) | Error handling macros for GUI context |
| [app/ipc.ts](../../app/ipc.ts) | Transport abstraction, `isTauri()` detection |
| [app/stores.ts](../../app/stores.ts) | Event-backed stores (dual-mode) |

## Runtime Detection

The frontend detects its environment via:

```typescript
export function isTauri(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
```

This check is used to:
- Select Tauri IPC or HTTP transport
- Show native vs HTML context menus  
- Enable keyboard accelerators (Tauri-only)
- Set up bidirectional event listeners

## IPC Transport

All backend communication flows through `app/ipc.ts`. In GUI mode, the `call()` function uses Tauri's `invoke()`:

```typescript
const { invoke } = await import("@tauri-apps/api/core");
return invoke<T>(command, args);
```

Each command maps to a `#[tauri::command]` handler in [src/gui/mod.rs](../../src/gui/mod.rs):

| Function | Tauri Command | Purpose |
|----------|---------------|---------|
| `query()` | `query_*` | Request readonly data |
| `mutate()` | `*_revision`, `*_ref`, etc. | Modify repository state |
| `trigger()` | `forward_*` | Fire-and-forget backend actions |

### Request/Response Flow

1. Frontend calls `query()`, `mutate()`, or `trigger()`
2. `call()` routes to Tauri `invoke()`
3. Command handler retrieves `SessionEvent` channel from `AppState`
4. Worker processes event, sends response via oneshot channel
5. Handler returns result to frontend
6. For mutations: frontend updates stores from response

## Event System

GUI mode uses Tauri's bidirectional event system for push updates:

### Backend → Frontend Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `gg://repo/config` | `RepoConfig` | Workspace opened, worker error |
| `gg://repo/status` | `RepoStatus` | Window focused (snapshot), mutation completed |
| `gg://menu/revision` | `string` | Menu item selected |

Events are emitted via `window.emit_to()`:

```rust
window.emit_to(
    EventTarget::labeled(window.label()),
    "gg://repo/status",
    status
)
```

### Frontend → Backend Events

| Event | Payload | Purpose |
|-------|---------|---------|
| `gg://revision/select` | `RevHeader \| null` | Update menu item enabled state |

The frontend listens and emits events via stores created with `event()` from `ipc.ts`:

```typescript
export const repoStatusEvent = await event<RepoStatus | undefined>("gg://repo/status", undefined);
```

## Window Management

### Multi-Window Support

Each window is independent with its own:
- Worker thread (owns `Session`)
- mpsc channel for `SessionEvent`s
- Context menus (revision, tree, ref)
- Window state (position, size via `tauri-plugin-window-state`)

Windows are keyed by a hash of the workspace path:

```rust
fn label_for_path(path: Option<&PathBuf>) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("repo-{:08x}", hasher.finish() as u32)
}
```

Opening the same workspace twice focuses the existing window.

### Window Lifecycle

1. **Creation**: `try_create_window()` builds a `WebviewWindow` with plugins
2. **Setup**: `setup_window()` spawns worker thread, registers event handlers
3. **Focus**: Triggers snapshot refresh via `SessionEvent::ExecuteSnapshot`
4. **Destruction**: Worker channel dropped, thread exits, state removed

## Native Menus

### Main Menu Bar

Built in `menu::build_main()` with platform-specific items:
- **macOS**: App menu with About, Services, Hide, Quit
- **All platforms**: Repository, Revision, Edit menus

The Revision menu is dynamically enabled/disabled based on `gg://revision/select` events.

### Context Menus

Three context menus built in `menu::build_context()`:
- **Revision menu**: New child, edit, duplicate, abandon, squash, etc.
- **Tree menu**: Squash, restore operations on changes
- **Ref menu**: Track, untrack, push, fetch, rename, delete branches

Context menus are shown via `window.popup_menu()` after enabling appropriate items based on the operand.

### Menu → Action Flow

1. User right-clicks object → `forward_context_menu` command
2. `handle_context()` enables menu items, calls `window.popup_menu()`
3. User selects item → `handle_event()` emits `gg://menu/*` event
4. Frontend receives event, calls appropriate mutator

## Plugins

GUI mode uses several Tauri plugins:

| Plugin | Purpose |
|--------|---------|
| `tauri-plugin-shell` | Open URLs, spawn processes |
| `tauri-plugin-dialog` | Native file/folder picker dialogs |
| `tauri-plugin-window-state` | Persist window size/position |
| `tauri-plugin-log` | Structured logging with level control |

## Worker Thread

Each window spawns a dedicated worker thread:

```rust
let window_worker = thread::spawn(move || {
    async_runtime::block_on(work(handle, receiver, workspace, settings))
});
```

The worker loops on `WorkerSession::handle_events()`, processing `SessionEvent`s from the IPC commands. If the worker encounters an unrecoverable error, it restarts and emits `gg://repo/config` with `WorkerError`.

## Accelerators

Global keyboard shortcuts are forwarded via `forward_accelerator`:

```rust
#[tauri::command]
fn forward_accelerator(window: Window, key: char) {
    if key == 'o' {
        menu::repo_open(&window);
    }
}
```

Menu items define accelerators like `"cmdorctrl+o"` for cross-platform support.

## Recent Workspaces

GUI mode maintains a recent workspaces list:

1. On workspace open, `add_recent_workspaces()` updates `gg.ui.recent-workspaces` config
2. List is truncated to 10 entries
3. On Windows, also updates the taskbar jump list

The update runs in a background thread to avoid blocking the UI.

## Platform-Specific Behavior

### macOS
- Dock icon set via `crate::macos::set_dock_icon()` when running as CLI
- App menu includes standard macOS items (About, Services, Hide, Quit)
- Native context menus via `popup_menu()`

### Windows  
- Drag-and-drop disabled on `WebviewWindow` (handled differently)
- Jump list updated with recent workspaces

## Launching GUI Mode

```bash
# Default (when gg.default-mode = "gui")
cargo run

# Explicit
cargo run -- gui

# With specific workspace
cargo run -- gui --workspace /path/to/repo

# Debug logging
cargo tauri dev -- -- --debug
```

## Adding New Commands

To add a new IPC command for GUI mode:

1. **Define message types** in `src/messages/` with `#[cfg_attr(feature = "ts-rs", derive(TS))]`
2. **Add handler** in `src/gui/mod.rs`:
   ```rust
   #[tauri::command(async)]
   fn my_command(
       window: Window,
       app_state: State<AppState>,
       args: MyArgs,
   ) -> Result<MyResult, InvokeError> {
       // ...
   }
   ```
3. **Register** in `invoke_handler!` macro
4. **Run** `cargo gen` to export TypeScript types
5. **Call** from frontend via `query()`, `mutate()`, or `trigger()`
