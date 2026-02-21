<script lang="ts">
    import type { Route } from "./route.js";
    import type { RevSet } from "./messages/RevSet";
    import type { RevsResult } from "./messages/RevsResult";
    import type { RepoConfig } from "./messages/RepoConfig";
    import type { RepoStatus } from "./messages/RepoStatus";
    import { type Query, query, mutate, trigger, onEvent, isTauri, getInput } from "./ipc.js";
    import {
        currentMutation,
        currentContext,
        repoConfigEvent,
        repoStatusEvent,
        revisionSelectEvent,
        selectionHeaders,
        currentInput,
        hasMenu,
        progressEvent,
        lastFocus,
        ignoreToggled,
    } from "./stores.js";
    import ContextMenu from "./controls/ContextMenu.svelte";
    import RefMutator from "./mutators/RefMutator";
    import ChangeMutator from "./mutators/ChangeMutator";
    import RevisionMutator from "./mutators/RevisionMutator";
    import Pane from "./shell/Pane.svelte";
    import Zone from "./objects/Zone.svelte";
    import StatusBar from "./shell/StatusBar.svelte";
    import ModalOverlay from "./shell/ModalOverlay.svelte";
    import ErrorDialog from "./shell/ErrorDialog.svelte";
    import ProgressDialog from "./shell/ProgressDialog.svelte";
    import RecentWorkspaces from "./shell/RecentWorkspaces.svelte";
    import { onMount, setContext } from "svelte";
    import InputDialog from "./shell/InputDialog.svelte";
    import type Settings from "./shell/Settings";
    import type { RepoEvent } from "./messages/RepoEvent";
    import RepositoryMutator from "./mutators/RepositoryMutator";

    interface $$Slots {
        default: {
            workspace: Extract<RepoConfig, { type: "Workspace" }>;
            selection: Query<RevsResult>;
        };
    }

    export let route: Route;
    export let revsetOverride: string | null = null;

    let selection: Query<RevsResult> = {
        type: "wait",
    };
    // for open recent workspaces when error dialogs happen
    let recentWorkspaces: string[] = [];

    if (isTauri()) {
        document.addEventListener("keydown", (event) => {
            const key = event.key.toLowerCase();
            if ((key === "n" || key === "m" || key === "o") && event.ctrlKey) {
                event.preventDefault();
                trigger("forward_accelerator", {
                    key,
                    ctrl: event.ctrlKey,
                    shift: event.shiftKey,
                });
            }
        });
    }

    document.body.addEventListener("click", () => currentContext.set(null), true);

    async function initialize() {
        const result = await query<RepoConfig>("query_workspace", null);
        if ($repoConfigEvent.type === "TimeoutError") {
            return; // too late! we're dead!
        } else if (result.type === "data") {
            repoConfigEvent.set(result.value);
        } else {
            repoConfigEvent.set({
                type: "LoadError",
                absolute_path: "",
                message: result.message,
            });
        }
    }
    initialize();

    let loadTimeout: number | null;
    onMount(() => {
        if ($repoConfigEvent.type == "Initial") {
            loadTimeout = setTimeout(() => {
                repoConfigEvent.set({ type: "TimeoutError" });
            }, 10_000);
        }

        if (!isTauri()) {
            // signal shutdown on unload, cancel if it was a reload
            const handleUnload = () => {
                trigger("begin_shutdown");
            };
            trigger("end_shutdown");
            window.addEventListener("beforeunload", handleUnload);

            // snapshot when focusing the browser or returning to the tab
            document.addEventListener("visibilitychange", handleFocus);
            window.addEventListener("focus", handleFocus);

            // ping the backend; if either component dies, the other can clean up
            const heartbeatInterval = setInterval(async () => {
                trigger("heartbeat", undefined, cleanup);
            }, 30_000);

            const cleanup = () => {
                clearInterval(heartbeatInterval);
                document.removeEventListener("visibilitychange", handleFocus);
                window.removeEventListener("focus", handleFocus);
                window.removeEventListener("beforeunload", handleUnload);
            };
            return cleanup;
        }
    });

    let settings: Settings = {
        markUnpushedBookmarks: true,
    };
    setContext<Settings>("settings", settings);

    if (isTauri()) {
        // web mode: mutations done directly by ContextMenu
        // gui mode: mutations triggered by native context menu
        onEvent("gg://context/revision", mutateRevision);
        onEvent("gg://context/tree", mutateTree);
        onEvent("gg://context/bookmark", mutateRef);
        onEvent("gg://menu/repo", mutateRepository);
        // gui mode: snapshot when window gains focus
        onEvent("gg://focus", handleFocus);
    }

    $: if ($repoConfigEvent) loadRepo($repoConfigEvent);
    $: if ($repoStatusEvent && revsetOverride && !$revisionSelectEvent) {
        let syntheticId = {
            change: {
                type: "ChangeId" as const,
                hex: revsetOverride,
                prefix: revsetOverride,
                rest: "",
                offset: null,
                is_divergent: false,
            },
            commit: { type: "CommitId" as const, hex: "", prefix: "", rest: "" },
        };
        $revisionSelectEvent = { from: syntheticId, to: syntheticId };
    }
    $: if ($repoStatusEvent && $revisionSelectEvent && route.type !== "log") {
        loadChange($revisionSelectEvent);
    }
    $: if (!isTauri()) {
        document.title =
            $repoConfigEvent.type === "Workspace"
                ? "GG - " + $repoConfigEvent.absolute_path.split("/").pop()
                : "GG - Gui for JJ";
    }
    $: if (
        $repoConfigEvent.type === "LoadError" ||
        $repoConfigEvent.type === "TimeoutError" ||
        $repoConfigEvent.type === "WorkerError"
    ) {
        queryRecentWorkspaces();
    }

    async function loadRepo(config: RepoConfig) {
        if (loadTimeout) {
            clearTimeout(loadTimeout);
            loadTimeout = null;
        }

        $revisionSelectEvent = undefined;
        if (config.type == "Workspace") {
            settings.markUnpushedBookmarks = config.mark_unpushed_bookmarks;
            ignoreToggled.set(config.ignore_immutable);
            $repoStatusEvent = config.status;
        }
    }

    async function loadChange(set: RevSet) {
        let rev = await query<RevsResult>("query_revisions", { set }, (q) => (selection = q));

        // if empty, fall back to working copy
        if (
            rev.type == "data" &&
            rev.value.type == "NotFound" &&
            (set.from.commit.hex !== $repoStatusEvent?.working_copy?.hex ||
                set.to.commit.hex !== $repoStatusEvent?.working_copy?.hex)
        ) {
            const workingCopyId = {
                change: {
                    type: "ChangeId" as const,
                    hex: "@",
                    prefix: "@",
                    rest: "",
                    offset: null,
                    is_divergent: false,
                },
                commit: $repoStatusEvent!.working_copy,
            };
            return loadChange({ from: workingCopyId, to: workingCopyId });
        }

        selection = rev;
        if (rev.type == "data" && rev.value.type == "Detail") {
            $selectionHeaders = rev.value.headers;
        }
    }

    async function handleFocus() {
        if (!isTauri() && document.visibilityState !== "visible") {
            return;
        }
        if ($repoConfigEvent.type === "Workspace") {
            const result = await query<RepoStatus | null>("query_snapshot", null);
            if (result.type === "data" && result.value) {
                repoStatusEvent.set(result.value);
            }
        }
        lastFocus.set(Date.now());
    }

    async function queryRecentWorkspaces() {
        const result = await query<string[]>("query_recent_workspaces", null);
        recentWorkspaces = result.type === "data" ? result.value : [];
    }

    function mutateRevision(event: string) {
        if ($currentContext?.type == "Revision") {
            new RevisionMutator([$currentContext.header], $ignoreToggled).handle(event);
        } else if ($currentContext?.type == "Revisions") {
            new RevisionMutator($currentContext.headers, $ignoreToggled).handle(event);
        }
        $currentContext = null;
    }

    function mutateTree(event: string) {
        if ($currentContext?.type == "Change") {
            new ChangeMutator(
                $currentContext.headers,
                $currentContext.path,
                $currentContext.hunk,
                $ignoreToggled,
            ).handle(event);
        }
        $currentContext = null;
    }

    function mutateRef(event: string) {
        if ($currentContext?.type == "Ref") {
            new RefMutator($currentContext.ref, $ignoreToggled).handle(event);
        }
        $currentContext = null;
    }

    function mutateRepository(event: RepoEvent) {
        new RepositoryMutator().handle(event);
    }
</script>

<Zone operand={{ type: "Repository" }} alwaysTarget let:target>
    <div id="shell" class={$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.theme_override : ""}>
        {#if $repoConfigEvent.type == "Initial"}
            <Pane>
                <h2 slot="header">Loading...</h2>
            </Pane>
        {:else if $repoConfigEvent.type == "Workspace"}
            <slot workspace={$repoConfigEvent} {selection} />
        {:else if $repoConfigEvent.type == "LoadError"}
            <ModalOverlay>
                <ErrorDialog title="No Workspace Loaded">
                    <p style="grid-column: 1/3">
                        You can run <code>gg</code> in a Jujutsu workspace or open one from the Repository menu.
                    </p>
                    <RecentWorkspaces workspaces={recentWorkspaces} />
                </ErrorDialog>
            </ModalOverlay>
        {:else if $repoConfigEvent.type == "TimeoutError"}
            <ModalOverlay>
                <ErrorDialog title="No Workspace Loaded" severe>
                    <p>Error communicating with backend: the operation is taking too long.</p>
                    <p>You may need to restart GG to continue.</p>
                    <RecentWorkspaces workspaces={recentWorkspaces} />
                </ErrorDialog>
            </ModalOverlay>
        {:else}
            <ModalOverlay>
                <ErrorDialog title="Fatal Error" severe>
                    <p>Error communicating with backend: {$repoConfigEvent.message}.</p>
                    <p>You may need to restart GG to continue.</p>
                    <RecentWorkspaces workspaces={recentWorkspaces} />
                </ErrorDialog>
            </ModalOverlay>
        {/if}

        <div class="separator" style="grid-row: 2"></div>

        <StatusBar {target} />

        {#if $currentInput}
            <ModalOverlay>
                <InputDialog
                    title={$currentInput.title}
                    detail={$currentInput.detail}
                    fields={$currentInput.fields}
                    on:response={(event) => $currentInput?.callback(event.detail)} />
            </ModalOverlay>
        {:else if $currentMutation}
            <ModalOverlay>
                {#if $currentMutation.type == "wait" && $progressEvent !== undefined}
                    <ProgressDialog progress={$progressEvent} />
                {:else if $currentMutation.type == "data" && ($currentMutation.value.type == "InternalError" || $currentMutation.value.type == "PreconditionError")}
                    <ErrorDialog title="Command Error" onClose={() => ($currentMutation = null)} severe>
                        {#if $currentMutation.value.type == "InternalError"}
                            <p>
                                {#each $currentMutation.value.message.lines as line}
                                    {line}<br />
                                {/each}
                            </p>
                        {:else}
                            <p>{$currentMutation.value.message}</p>
                        {/if}
                    </ErrorDialog>
                {:else if $currentMutation.type == "error"}
                    <ErrorDialog title="IPC Error" onClose={() => ($currentMutation = null)} severe>
                        <p>{$currentMutation.message}</p>
                    </ErrorDialog>
                {/if}
            </ModalOverlay>
        {/if}

        {#if $currentContext && $hasMenu}
            <ContextMenu
                operand={$currentContext}
                x={$hasMenu.x}
                y={$hasMenu.y}
                onClose={() => {
                    hasMenu.set(null);
                    currentContext.set(null);
                }} />
        {/if}
    </div>
</Zone>

<style>
    #shell {
        width: 100vw;
        height: 100vh;

        display: grid;
        grid-template-columns: 1fr;
        grid-template-rows: 1fr 3px 30px;
        grid-template-areas:
            "content"
            "."
            "footer";

        background: var(--ctp-crust);
        color: var(--ctp-text);

        user-select: none;
    }

    .separator {
        background: var(--ctp-overlay0);
    }

    p {
        pointer-events: auto;
        user-select: text;
    }
</style>
