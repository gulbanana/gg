import { vi } from "vitest";
import { cleanup } from "@testing-library/svelte";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";

export type IpcHandler = (cmd: string, args: Record<string, unknown>) => unknown;

// should be called before any imports that check isTauri()
export function setupMocks(ipcHandler?: IpcHandler) {
    (window as any).__TAURI_INTERNALS__ = {
        invoke: vi.fn(),
        metadata: { currentWindow: { label: "main" } },
    };

    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        _listeners: new Map(),
    };

    mockIPC((cmd, args) => {
        if (cmd === "plugin:event|listen") {
            return 1;
        }
        return ipcHandler?.(cmd, args as Record<string, unknown>);
    });
}

export async function cleanupMocks() {
    // unmount components before clearing mocks to avoid cleanup errors
    cleanup();
    clearMocks();

    // reset to minimal valid state instead of deleting - lingering async operations
    // (like dynamic imports of @tauri-apps/api) may still try to access these
    (window as any).__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "" } },
        invoke: () => Promise.resolve(),
        transformCallback: () => 0,
    };
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        _listeners: new Map(),
        unregisterListener: () => {},
    };
    vi.resetModules();
}
