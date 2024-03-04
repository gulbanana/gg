import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event"
import type { Readable, Subscriber, Unsubscriber } from "svelte/store";

export type Query<T> = { type: "wait" } | { type: "data", value: T } | { type: "error", message: string };

class EventStore<T> implements Readable<T | undefined> {
    #name: string;

    constructor(name: string) {
        this.#name = name;
    }

    subscribe(run: Subscriber<T>): Unsubscriber {
        let unlisten = listen<T>(this.#name, event => run(event.payload));
        return async () => (await unlisten)();
    }

    set(value: T) {
        emit(this.#name, value);
    }
}

class CommandStore<T> implements Readable<Query<T>> {
    #name: string;
    #response: Query<T>;
    #subscribers = new Set<(result: Query<T>) => void>();

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

export function event<T>(name: string): EventStore<T> {
    return new EventStore(name);
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
