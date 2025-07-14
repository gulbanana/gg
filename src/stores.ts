import type { MutationResult } from "./messages/MutationResult";
import type { RepoConfig } from "./messages/RepoConfig";
import type { RepoStatus } from "./messages/RepoStatus";
import type { RevHeader } from "./messages/RevHeader";
import type { Operand } from "./messages/Operand";
import { writable } from "svelte/store";
import { event, type Query } from "./ipc";
import type { InputRequest } from "./messages/InputRequest";
import type { InputResponse } from "./messages/InputResponse";
import type { RevChange } from "./messages/RevChange";
import type { ChangeId } from "./messages/ChangeId";

export const repoConfigEvent = await event<RepoConfig>("gg://repo/config", { type: "Initial" });
export const repoStatusEvent = await event<RepoStatus | undefined>("gg://repo/status", undefined);
export const revisionSelectEvent = await event<RevHeader | undefined>("gg://revision/select", undefined);
export const changeSelectEvent = await event<RevChange | undefined>("gg://change/select", undefined);

export const currentRevisionSet = writable<Set<ChangeId>>(new Set());
export const currentRevisionSetHex = writable<Set<string>>(new Set());

export const currentMutation = writable<Query<MutationResult> | null>(null);
export const currentContext = writable<Operand | null>();
export const currentSource = writable<Operand | null>();
export const currentTarget = writable<Operand | null>();
export const currentInput = writable<InputRequest & { callback: (response: InputResponse) => void } | null>();

export const hasModal = writable<boolean>(false);

export function dragOverWidget(event: DragEvent) {
    event.stopPropagation();
    currentTarget.set(null);
}