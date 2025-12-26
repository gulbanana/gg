import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { emit, listen, type EventCallback } from "@tauri-apps/api/event";
import type { Readable, Subscriber, Unsubscriber } from "svelte/store";
import type { MutationResult } from "./messages/MutationResult";
import { currentInput, currentMutation, repoStatusEvent, revisionSelectEvent } from "./stores";
import { onMount } from "svelte";
import { resolve } from "@tauri-apps/api/path";

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
type ImmediateQuery<T> = Extract<Query<T>, { type: "data" } | { type: "error" }>;
type DelayedQuery<T> = Extract<Query<T>, { type: "wait" }>;
export async function query<T>(command: string, request: InvokeArgs | null, onWait?: (q: DelayedQuery<T>) => void): Promise<ImmediateQuery<T>> {
    try {
        if (onWait) {
            let fetch = invoke<T>(command, request ?? undefined).then(value => ({ type: "data", value } as ImmediateQuery<T>));
            let result = await Promise.race([fetch, delay<T>()]);
            if (result.type == "wait") {
                onWait(result);
                result = await fetch;
            }
            return result;
        } else {
            let result = await invoke<T>(command, request ?? undefined);
            return { type: "data", value: result };
        }
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
export async function mutate<T>(command: string, mutation: T): Promise<boolean> {
    try {
        // set a wait state then the data state, unless the data comes in hella fast
        let fetch = invoke<MutationResult>(command, { mutation });
        let result = await Promise.race([fetch.then(r => Promise.resolve<Query<MutationResult>>({ type: "data", value: r })), delay<MutationResult>()]);
        currentMutation.set(result);
        let value = await fetch;

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
        if (value.type == "Updated" || value.type == "UpdatedSelection" || value.type == "Unchanged") {
            if (value.type != "Unchanged") {
                repoStatusEvent.set(value.new_status);
                if (value.type == "UpdatedSelection") {
                    revisionSelectEvent.set(value.new_selection);
                }
            }
            currentMutation.set(null);
            return true;
        }

        // failed; transition from overlay or delay to error
        currentMutation.set({ type: "data", value });
        return false;
    } catch (error: any) {
        console.log(error);
        currentMutation.set({ type: "error", message: error.toString() });
        return false;
    }
}

/**
 * utility function for composing IPCs with delayed loading states
 */
export function delay<T>(): Promise<Query<T>> {
    return new Promise(function (resolve) {
        setTimeout(() => resolve({ type: "wait" }), 250);
    });
}

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