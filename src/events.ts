import type { RepoConfig } from "./messages/RepoConfig";
import type { RepoStatus } from "./messages/RepoStatus";
import type { RevHeader } from "./messages/RevHeader";
import { event } from "./ipc";

export let repoConfig = await event<RepoConfig>("gg://repo/config");
export let repoStatus = await event<RepoStatus>("gg://repo/status");
export let revisionSelect = await event<RevHeader>("gg://revision/select");