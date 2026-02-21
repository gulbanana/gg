import { isTauri } from "./ipc.js";

export type Route =
    | { type: "default" }
    | { type: "log"; revset: string | null }
    | { type: "revision"; revset: string };

export function parseRoute(): Route {
    if (isTauri()) return { type: "default" };
    let params = new URLSearchParams(window.location.search);
    if (window.location.pathname === "/log")
        return { type: "log", revset: params.get("revset") };
    if (window.location.pathname === "/revision") {
        let revset = params.get("revset");
        if (revset) return { type: "revision", revset };
    }
    return { type: "default" };
}
