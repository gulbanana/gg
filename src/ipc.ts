import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { emit, listen, type EventCallback } from "@tauri-apps/api/event";
import type { Readable, Subscriber, Unsubscriber } from "svelte/store";
import type { MutationResult } from "./messages/MutationResult";
import { currentMutation, repoStatusEvent, revisionSelectEvent } from "./stores";
import { onMount } from "svelte";

export type Query<T> = { type: "wait" } | { type: "data", value: T } | { type: "error", message: string };

export interface Settable<T> extends Readable<T> {
    set: (value: T) => void;
}

/**
 * multiplexes tauri events into a svelte store; never actually unsubscribes because the store protocol isn't async
 */
export async function event<T>(name: string, initialValue: T): Promise<Settable<T>> {
    const subscribers = new Set<Subscriber<T>>();
    let lastValue: T = initialValue;

    const unlisten = await listen<T>(name, event => {
        for (let subscriber of subscribers) {
            subscriber(event.payload);
        }
    });

    return {
        subscribe(run: Subscriber<T>): Unsubscriber {
            // send current value to stream
            if (typeof lastValue != "undefined") {
                run(lastValue);
            }

            // listen for new values
            subscribers.add(run);

            return () => subscribers.delete(run);
        },

        set(value: T) {
            lastValue = value;
            emit(name, value);
        }
    }
}

/**
 * subscribes to tauri events for a component's lifetime
 */
export function onEvent<T>(name: string, callback: (payload: T) => void) {
    onMount(() => {
        let promise = listen<T>(name, e => callback(e.payload));
        return () => {
            promise.then((unlisten) => {
                unlisten();
            });
        };
    });
}

/**
 * call an IPC which provides readonly information about the repo
 */
export async function query<T>(command: string, request?: InvokeArgs): Promise<Query<T>> {
    // set a wait state then the data state, unless the data comes in hella fast
    try {
        let result = await invoke<T>(command, request);
        return { type: "data", value: result };
    } catch (error: any) {
        console.log(error);
        return { type: "error", message: error.toString() };
    }
}

/**
 * call an IPC which, if successful, has backend side-effects
 */
export function trigger(command: string, request?: InvokeArgs) {
    (async () => {
        try {
            await invoke(command, request);
        }
        catch (error: any) {
            console.log(error);
            currentMutation.set({ type: "error", message: error.toString() });
        }
    })();
}

/**
 * call an IPC which, if successful, modifies the repo
 */
export function mutate<T>(command: string, mutation: T) {
    (async () => {
        try {
            let fetch = invoke<MutationResult>(command, { mutation });
            let result = await Promise.race([fetch.then(r => Promise.resolve<Query<MutationResult>>({ type: "data", value: r })), delay<MutationResult>()]);
            currentMutation.set(result);
            let value = await fetch;

            // succeeded; dismiss modals
            if (value.type == "Updated" || value.type == "UpdatedSelection" || value.type == "Unchanged") {
                if (value.type != "Unchanged") {
                    repoStatusEvent.set(value.new_status);
                    if (value.type == "UpdatedSelection") {
                        revisionSelectEvent.set(value.new_selection);
                    }
                }
                currentMutation.set(null);

                // failed; transition from overlay or delay to error
            } else {
                currentMutation.set({ type: "data", value });
            }
        } catch (error: any) {
            console.log(error);
            currentMutation.set({ type: "error", message: error.toString() });
        }
    })();
}

/**
 * utility function for composing IPCs with delayed loading states
 */
export function delay<T>(): Promise<Query<T>> {
    return new Promise(function (resolve) {
        setTimeout(() => resolve({ type: "wait" }), 250);
    });
}
