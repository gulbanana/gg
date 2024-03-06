import type { MutationResult } from "./messages/MutationResult";
import type { RepoConfig } from "./messages/RepoConfig";
import type { RepoStatus } from "./messages/RepoStatus";
import type { RevHeader } from "./messages/RevHeader";
import { writable } from "svelte/store";
import { event, type Query } from "./ipc";

export const repoConfigEvent = await event<RepoConfig>("gg://repo/config");
export const repoStatusEvent = await event<RepoStatus>("gg://repo/status");
export const revisionSelectEvent = await event<RevHeader>("gg://revision/select");
export const menuCommitEvent = await event<string>("gg://menu/commit");
export const contextRevisionEvent = await event<string>("gg://context/revision");

export const currentMutation = writable<Query<MutationResult> | null>(null);
export const currentContext = writable<RevHeader | null>();