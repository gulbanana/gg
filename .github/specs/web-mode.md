# Web Mode Technical Specification

Web mode allows GG to run in a browser, served by an Axum HTTP server. This document describes the architecture and key implementation details.

## Overview

GG supports two launch modes:
- **GUI mode**: Tauri desktop app with native windowing and IPC
- **Web mode**: Axum server serving the frontend in a browser

Both modes share the same frontend code. The frontend detects its runtime environment and uses the appropriate transport layer.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Browser                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    Svelte Frontend                      │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │    │
│  │  │  stores.ts  │  │  ipc.ts     │  │  Components     │  │    │
│  │  │  (state)    │  │  (transport)│  │  (UI)           │  │    │
│  │  └─────────────┘  └──────┬──────┘  └─────────────────┘  │    │
│  └──────────────────────────┼──────────────────────────────┘    │
│                             │ HTTP POST                         │
└─────────────────────────────┼───────────────────────────────────┘
                              │
┌─────────────────────────────┼───────────────────────────────────┐
│                        Axum Server                              │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │                      /api/{cmd}                         │    │
│  │                    Route Handler                        │    │
│  └──────────────────────────┬──────────────────────────────┘    │
│                             │ mpsc channel                      │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │                    Worker Thread                        │    │
│  │              (same as GUI mode worker)                  │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Key Files

| File | Purpose |
|------|---------|
| [src/web/mod.rs](../../src/web/mod.rs) | Axum server, HTTP endpoints, worker spawning |
| [app/ipc.ts](../../app/ipc.ts) | Transport abstraction, `isTauri()` detection |
| [app/stores.ts](../../app/stores.ts) | Event-backed stores (dual-mode) |
| [app/controls/ContextMenu.svelte](../../app/controls/ContextMenu.svelte) | HTML context menu for web mode |

## Runtime Detection

The frontend detects its environment via:

```typescript
export function isTauri(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
```

This check is used to:
- Select HTTP or Tauri IPC transport
- Show HTML vs native context menus
- Enable/disable Tauri-only features (keyboard accelerators)

## IPC Transport

All backend communication flows through `app/ipc.ts`. In web mode:

| Function | HTTP Endpoint | Method | Purpose |
|----------|---------------|--------|----------|
| `query()` | `/api/query/{command}` | POST | Request readonly data |
| `mutate()` | `/api/mutate/{command}` | POST | Modify repository state |
| `trigger()` | `/api/trigger/{command}` | POST | Fire-and-forget backend actions |

The API endpoints in [src/web/mod.rs](../../src/web/mod.rs) route requests to the appropriate handler based on the command name.

### Request/Response Flow

1. Frontend calls `query()`, `mutate()`, or `trigger()`
2. `invokeCommand()` helper routes to HTTP fetch
3. Axum handler deserializes request, sends `SessionEvent` to worker
4. Worker processes event, sends response via channel
(for queries and mutations only)
5. Handler serializes response as JSON
6. Frontend receives and processes response
(for mutations only)
7. Frontend clears errors and timeouts.

## Event Stores

In GUI mode, stores like `repoStatusEvent` receive push updates via Tauri's event system. In web mode:

- **No push updates**: Stores are simple writables
- **Polling on focus**: `App.svelte` listens for `visibilitychange` and calls `query_snapshot`
- **Mutation responses**: `mutate()` updates `repoStatusEvent` from the response

The `event()` function in `ipc.ts` handles this transparently—it sets up Tauri listeners only when in GUI mode.

## Context Menus

| Mode | Implementation |
|------|----------------|
| GUI | Native popup via `forward_context_menu` → Rust → `gg://context/*` event |
| Web | HTML `<ContextMenu>` component rendered by `Object.svelte` |

The web mode context menu (`app/controls/ContextMenu.svelte`) calls mutator handlers directly, bypassing the event round-trip.

## Worker Thread

Web mode spawns a single worker thread identical to GUI mode:

```rust
thread::spawn(move || {
    tauri::async_runtime::block_on(async {
        WorkerSession::new(workspace, settings)
            .handle_events(&worker_rx)
            .await
    });
});
```

## Lifecycle Management

### Startup
1. `cargo run -- web` starts Axum server on random port
2. Server spawns worker thread
3. Opens browser to server URL
4. Frontend calls `query("query_workspace")` to load the initial workspace
5. Server opens workspace at CWD, returns `RepoConfig`

### Shutdown
Two mechanisms ensure the server doesn't run forever:

1. **Deferred beacon**: Frontend sends `navigator.sendBeacon('/api/trigger/begin_shutdown')` on `beforeunload`, which starts a 3-second grace period. If the page reloads within that window, `/api/trigger/end_shutdown` cancels the pending shutdown.
2. **Heartbeat timeout**: Frontend sends a heartbeat every 30 seconds via `/api/trigger/heartbeat`. Backend shuts down after 10 minutes without a heartbeat (handles browser crash or tab close when beacon fails).

### Crash Detection
The heartbeat also enables frontend detection of backend crashes. If a heartbeat fails, the frontend sets `repoConfigEvent` to `WorkerError` state, showing the fatal error dialog and stopping further backend communication attempts.

## Adding New Commands

To add a new IPC command that works in both modes:

1. **Rust**: Add handler in `src/gui/mod.rs` (Tauri command)
2. **Rust**: Add case in `handle_command()` match in `src/web/mod.rs`
3. **TypeScript**: Call via `query()`, `mutate()`, or `trigger()` as appropriate

The web mode handler should mirror the Tauri command's behavior.

## Testing Web Mode

```bash
# Start web mode
cargo run -- web

# Or with a specific workspace
cargo run -- web --workspace /path/to/repo
```

The server prints its URL and opens a browser. Check the browser console and terminal for errors.
