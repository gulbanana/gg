import type { MutationResult } from "./messages/MutationResult";
import type { ProgressEvent } from "./messages/ProgressEvent";
import type { RepoConfig } from "./messages/RepoConfig";
import type { RepoStatus } from "./messages/RepoStatus";
import type { RevSet } from "./messages/RevSet";
import type { Operand } from "./messages/Operand";
import { writable } from "svelte/store";
import { event, type Query } from "./events";
import type { InputRequest } from "./messages/InputRequest";
import type { InputResponse } from "./messages/InputResponse";
import type { RevChange } from "./messages/RevChange";

export const repoConfigEvent = await event<RepoConfig>("gg://repo/config", { type: "Initial" });
export const repoStatusEvent = await event<RepoStatus | undefined>("gg://repo/status", undefined);
export const revisionSelectEvent = await event<RevSet | undefined>("gg://revision/select", undefined);
export const changeSelectEvent = await event<RevChange | undefined>("gg://change/select", undefined);
export const progressEvent = await event<ProgressEvent | undefined>("gg://progress", undefined);

export const currentMutation = writable<Query<MutationResult> | null>(null);
export const currentContext = writable<Operand | null>();
export const currentSource = writable<Operand | null>();
export const currentTarget = writable<Operand | null>();
export const currentInput = writable<InputRequest & { callback: (response: InputResponse | null) => void } | null>();

export const hasModal = writable<boolean>(false);
export const hasMenu = writable<{ x: number; y: number } | null>(null);
export const lastFocus = writable<number>(Date.now());

export function dragOverWidget(event: DragEvent) {
    event.stopPropagation();
    currentTarget.set(null);
}