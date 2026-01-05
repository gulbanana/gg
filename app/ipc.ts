import type { MutationResult } from "./messages/MutationResult";
import { currentInput, currentMutation, progressEvent, repoConfigEvent, repoStatusEvent, revisionSelectEvent } from "./stores";
import { isTauri, type Query } from "./events";

export { isTauri, onEvent, type Query, type Settable } from "./events";

/** 
 * structurally equivalent to InvokeArgs from @tauri-apps/api/core
 */
export type InvokeArgs = Record<string, unknown>;

/**
 * call an IPC which provides readonly information about the repo
 */
type ImmediateQuery<T> = Extract<Query<T>, { type: "data" } | { type: "error" }>;
type DelayedQuery<T> = Extract<Query<T>, { type: "wait" }>;
export async function query<T>(command: string, request: InvokeArgs | null, onWait?: (q: DelayedQuery<T>) => void): Promise<ImmediateQuery<T>> {
    try {
        if (onWait) {
            let fetchPromise = call<T>("query", command, request ?? undefined).then(value => ({ type: "data", value } as ImmediateQuery<T>));
            let result = await Promise.race([fetchPromise, delay<T>()]);
            if (result.type == "wait") {
                onWait(result);
                result = await fetchPromise;
            }
            return result;
        } else {
            let result = await call<T>("query", command, request ?? undefined);
            return { type: "data", value: result };
        }
    } catch (error: any) {
        console.error(error);
        return { type: "error", message: error.toString() };
    }
}

/**
 * call an IPC which, if successful, has backend side-effects
 */
export function trigger(command: string, request?: InvokeArgs, onError?: () => void): void {
    (async () => {
        try {
            await call<void>("trigger", command, request);
        }
        catch (error: any) {
            console.error(error);
            repoConfigEvent.set({ type: "WorkerError", message: "Lost connection to server" });
            onError?.();
        }
    })();
}

/**
 * call an IPC which, if successful, modifies the repo
 */
export async function mutate<T>(command: string, mutation: T, options?: { operation?: string }): Promise<boolean> {
    if (options?.operation) {
        progressEvent.set({ type: "Message", text: options.operation });
    } else {
        progressEvent.set(undefined);
    }

    try {
        // set a wait state then the data state, unless the data comes in hella fast
        let fetchPromise = call<MutationResult>("mutate", command, { mutation });
        let result = await Promise.race([fetchPromise.then(r => Promise.resolve<Query<MutationResult>>({ type: "data", value: r })), delay<MutationResult>()]);
        currentMutation.set(result);
        let value = await fetchPromise;

        while (value.type == "InputRequired") {
            // dismiss loading overlay while showing input dialog
            currentMutation.set(null);
            const fields = await getInput(
                value.request.title,
                value.request.detail,
                value.request.fields
            );

            // display cancellation as error
            if (!fields) {
                currentMutation.set({
                    type: "data",
                    value: { type: "PreconditionError", message: "Authentication cancelled" }
                });
                return false;
            }

            // retry with input response
            const enhancedMutation = { ...mutation, input: { fields } };
            fetchPromise = call<MutationResult>("mutate", command, { mutation: enhancedMutation });
            result = await Promise.race([fetchPromise.then(r => Promise.resolve<Query<MutationResult>>({ type: "data", value: r })), delay<MutationResult>()]);
            currentMutation.set(result);
            value = await fetchPromise;
        }

        // succeeded; dismiss modals
        if (value.type == "Unchanged" || value.type == "Updated" || value.type == "Reconfigured") {
            if (value.type == "Reconfigured") {
                repoConfigEvent.set(value.new_config);
            } else if (value.type == "Updated") {
                repoStatusEvent.set(value.new_status);
                if (value.new_selection) {
                    revisionSelectEvent.set({
                        from: value.new_selection.id,
                        to: value.new_selection.id,
                    });
                }
            }
            currentMutation.set(null);
            return true;
        }

        // failed; transition from overlay or delay to error
        currentMutation.set({ type: "data", value });
        return false;
    } catch (error: any) {
        console.error(error);
        currentMutation.set({ type: "error", message: error.toString() });
        return false;
    }
}

/**
 * utility function for composing IPCs with delayed loading states
 */
export function delay<T>(): Promise<Query<T>> {
    return new Promise(function (resolve) {
        setTimeout(() => resolve({ type: "wait" }), 500);
    });
}

/**
 * not actually IPC, just opens a modal
 */
export function getInput<const T extends string>(title: string, detail: string, fields: T[] | { label: T, choices: string[] }[]): Promise<{ [K in T]: string } | null> {
    return new Promise(resolve => {
        if (typeof fields[0] == "string") {
            fields = fields.map(f => ({ label: f, choices: [] } as { label: T, choices: string[] }));
        }
        currentInput.set({
            title, detail, fields: fields as { label: T, choices: string[] }[], callback: response => {
                currentInput.set(null);
                resolve(response ? response.fields as any : null);
            }
        });
    });
}

/**
 * id should be injected by the server, but a random one is ok as long as it's consistent
 */
function getClientId(): string {
    if (!window.__GG_CLIENT_ID__) {
        window.__GG_CLIENT_ID__ = crypto.randomUUID();
    }
    return window.__GG_CLIENT_ID__;
}

/**
 * route to Tauri or HTTP based on runtime environment
 */
async function call<T>(mode: "query" | "mutate" | "trigger", command: string, args?: InvokeArgs): Promise<T> {
    if (isTauri()) {
        const { invoke } = await import("@tauri-apps/api/core");
        return invoke<T>(command, args);
    } else {
        if (mode == "trigger") {
            const payload = { ...args, client_id: getClientId() };
            const blob = new Blob([JSON.stringify(payload)], { type: 'application/json' });
            navigator.sendBeacon(`/api/${mode}/${command}`, blob);
            return undefined as T;
        } else {
            const response = await fetch(`/api/${mode}/${command}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(args ?? {})
            });
            if (!response.ok) {
                const errorText = await response.text();
                throw new Error(errorText || `HTTP ${response.status}`);
            }
            return response.json();
        }
    }
}
