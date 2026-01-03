import type { Readable, Subscriber, Unsubscriber } from "svelte/store";
import { onMount } from "svelte";

export type Query<T> = { type: "wait" } | { type: "data", value: T } | { type: "error", message: string };

type SseListener<T = unknown> = (payload: T) => void;
const sseListeners = new Map<string, Set<SseListener>>();
let sseConnection: EventSource | null = null;

function getSseConnection(): EventSource {
    if (!sseConnection) {
        sseConnection = new EventSource('/api/events');
    }
    return sseConnection;
}

function addSseListener<T>(eventName: string, listener: SseListener<T>): () => void {
    // lazy init on first use 
    if (!sseListeners.has(eventName)) {
        sseListeners.set(eventName, new Set());

        getSseConnection().addEventListener(eventName, (e: MessageEvent) => {
            const payload = JSON.parse(e.data);
            for (const cb of sseListeners.get(eventName) ?? []) {
                cb(payload);
            }
        });
    }

    sseListeners.get(eventName)!.add(listener as SseListener);

    return () => sseListeners.get(eventName)?.delete(listener as SseListener);
}

export interface Settable<T> extends Readable<T> {
    set: (value: T) => void;
}

export function isTauri(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * multiplexes events into a svelte store; never actually unsubscribes because the store protocol isn't async.
 * gui mode: events are broadcast to and received from the backend via Tauri
 * web mode: events are received from the backend via SSE, set() is local-only
 */
export async function event<T>(name: string, initialValue: T): Promise<Settable<T>> {
    const subscribers = new Set<Subscriber<T>>();
    let lastValue: T = initialValue;

    if (isTauri()) {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().listen<T>(name, event => {
            lastValue = event.payload;
            for (let subscriber of subscribers) {
                subscriber(event.payload);
            }
        });
    } else {
        addSseListener<T>(name, payload => {
            lastValue = payload;
            for (let subscriber of subscribers) {
                subscriber(payload);
            }
        });
    }

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
            for (let subscriber of subscribers) {
                subscriber(value);
            }
            if (isTauri()) {
                (async () => {
                    const { emitTo } = await import("@tauri-apps/api/event");
                    const { getCurrentWindow } = await import("@tauri-apps/api/window");
                    emitTo(getCurrentWindow().label, name, value);
                })();
            }
        }
    }
}

/**
 * subscribe to backend events for a component's lifetime.
 * gui mode: subscribes to Tauri events
 * web mode: subscribes to SSE events
 */
export function onEvent<T>(name: string, callback: (payload: T) => void) {
    onMount(() => {
        if (isTauri()) {
            let promise = import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
                getCurrentWindow().listen<T>(name, e => callback(e.payload))
            );
            return () => {
                promise.then((unlisten) => {
                    unlisten();
                });
            };
        } else {
            return addSseListener<T>(name, callback);
        }
    });
}