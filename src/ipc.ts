import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event"
import type { Readable, Subscriber, Unsubscriber } from "svelte/store";

export type Query<T> = { type: "wait" } | { type: "data", value: T } | { type: "error", message: string };

export interface Settable<T> extends Readable<T> {
    set: (value: T) => void;
}

// multiplexes tauri events into a svelte store; never actually unsubscribes because the store protocol isn't async
export async function event<T>(name: string): Promise<Settable<T | undefined>> {
    const subscribers = new Set<Subscriber<T>>();
    let lastValue: T | undefined;

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

        set(value: T | undefined) {
            lastValue = value;
            emit(name, value);
        }
    }
}

class CommandStore<T> implements Readable<Query<T>> {
    #name: string;
    #response: Query<T>;
    #subscribers = new Set<Subscriber<Query<T>>>();

    constructor(name: string, initialData?: T) {
        this.#name = name;
        this.reset(initialData);
    }

    subscribe(run: Subscriber<Query<T>>): Unsubscriber {
        // send current value to stream
        run(this.#response);

        // listen for new values
        this.#subscribers.add(run);

        return () => this.#subscribers.delete(run);
    }

    reset(initialData?: T) {
        if (typeof (initialData) == "undefined") {
            this.#response = { type: "wait" };
        } else {
            this.#response = { type: "data", value: initialData };
        }
    }

    async call(request: InvokeArgs): Promise<Query<T>> {
        // set a wait state then the data state, unless the data comes in hella fast
        try {
            let fetch = invoke<T>(this.#name, request).then<Query<T>>(result => { return { type: "data", value: result }; });
            this.#response = await Promise.race([fetch, delay<T>(200)]);
            if (this.#response.type == "wait") {
                this.#response = await fetch;
            }
        } catch (error: any) {
            console.log(error);
            this.#response = { type: "error", message: error.toString() };
        }

        // notify all listeners
        for (let subscriber of this.#subscribers) {
            subscriber(this.#response);
        }

        // return to caller for immediate use
        return this.#response;
    }
}

export function command<T>(name: string, initialData?: T): CommandStore<T> {
    return new CommandStore(name, initialData);
}

export async function call<T>(name: string, request?: InvokeArgs): Promise<Query<T>> {
    // set a wait state then the data state, unless the data comes in hella fast
    try {
        let result = await invoke<T>(name, request);
        return { type: "data", value: result };
    } catch (error: any) {
        console.log(error);
        return { type: "error", message: error.toString() };
    }
}

export function delay<T>(ms: number): Promise<Query<T>> {
    return new Promise(function (resolve) {
        setTimeout(() => resolve({ type: "wait" }), ms);
    });
}
