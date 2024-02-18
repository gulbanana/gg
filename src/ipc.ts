import { invoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";

export type IPC<T> = { type: "wait" } | { type: "data", value: T } | { type: "error", message: string };

export function init<T>(initialData?: T): IPC<T> {
    if (typeof (initialData) == "undefined") {
        return { type: "wait" };
    } else {
        return { type: "data", value: initialData };
    }
}

// invokes with a result wrapper and (someday) progress indicators
export async function call<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<IPC<T>> {
    try {
        let result = await invoke<T>(cmd, args, options);
        return { type: "data", value: result };
    } catch (error: any) {
        console.log(error);
        return { type: "error", message: error.toString() };
    }
}

export function delayInit<T>(): Promise<IPC<T>> {
    return new Promise(function (resolve) {
        setTimeout(() => resolve({ type: "wait" }), 200);
    });
}