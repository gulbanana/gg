<script lang="ts">
    import type { RevId } from "./messages/RevId";
    import type { RevResult } from "./messages/RevResult";
    import type { RepoConfig } from "./messages/RepoConfig";
    import { type Query, query, trigger, onEvent } from "./ipc.js";
    import {
        currentMutation,
        currentContext,
        repoConfigEvent,
        repoStatusEvent,
        revisionSelectEvent,
        currentInput,
    } from "./stores.js";
    import RefMutator from "./mutators/RefMutator";
    import ChangeMutator from "./mutators/ChangeMutator";
    import RevisionMutator from "./mutators/RevisionMutator";
    import Pane from "./shell/Pane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import LogPane from "./LogPane.svelte";
    import BoundQuery from "./controls/BoundQuery.svelte";
    import Zone from "./objects/Zone.svelte";
    import StatusBar from "./shell/StatusBar.svelte";
    import ModalOverlay from "./shell/ModalOverlay.svelte";
    import ErrorDialog from "./shell/ErrorDialog.svelte";
    import RecentWorkspaces from "./shell/RecentWorkspaces.svelte";
    import { onMount, setContext } from "svelte";
    import IdSpan from "./controls/IdSpan.svelte";
    import InputDialog from "./shell/InputDialog.svelte";
    import type { InputRequest } from "./messages/InputRequest";
    import type { InputResponse } from "./messages/InputResponse";
    import type Settings from "./shell/Settings";

    let selection: Query<RevResult> = {
        type: "wait",
    };
    // for open recent workspaces when error dialogs happen
    let recentWorkspaces: string[] = [];

    document.addEventListener("keydown", (event) => {
        if (event.key === "o" && event.ctrlKey) {
            event.preventDefault();
            trigger("forward_accelerator", { key: "o" });
        }
    });

    document.body.addEventListener("click", () => currentContext.set(null), true);

    // this is a special case - most triggers are fire-and-forget, but we really need a
    // gg://repo/config event in response to this one. if it takes too long, we make our own
    trigger("notify_window_ready");
    let loadTimeout: number | null;
    onMount(() => {
        if ($repoConfigEvent.type == "Initial") {
            loadTimeout = setTimeout(() => {
                repoConfigEvent.set({ type: "TimeoutError" });
            }, 10_000);
        }
    });

    let settings: Settings = {
        markUnpushedBranches: true,
    };
    setContext<Settings>("settings", settings);

    onEvent("gg://context/revision", mutateRevision);
    onEvent("gg://context/tree", mutateTree);
    onEvent("gg://context/branch", mutateRef);
    onEvent("gg://input", requestInput);

    $: if ($repoConfigEvent) loadRepo($repoConfigEvent);
    $: if ($repoStatusEvent && $revisionSelectEvent) loadChange($revisionSelectEvent.id);
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
            settings.markUnpushedBranches = config.mark_unpushed_branches;
            $repoStatusEvent = config.status;
        }
    }

    async function loadChange(id: RevId) {
        let rev = await query<RevResult>("query_revision", { id }, (q) => (selection = q));

        if (
            rev.type == "data" &&
            rev.value.type == "NotFound" &&
            id.commit.hex != $repoStatusEvent?.working_copy.hex
        ) {
            return loadChange({
                change: { type: "ChangeId", hex: "@", prefix: "@", rest: "" },
                commit: $repoStatusEvent!.working_copy,
            });
        }

        selection = rev;
    }

    async function queryRecentWorkspaces() {
        const result = await query<string[]>("query_recent_workspaces", null);
        recentWorkspaces = result.type === "data" ? result.value : [];
    }

    function mutateRevision(event: string) {
        if ($currentContext?.type == "Revision") {
            new RevisionMutator($currentContext.header).handle(event);
        }
        $currentContext = null;
    }

    function mutateTree(event: string) {
        if ($currentContext?.type == "Change") {
            new ChangeMutator($currentContext.header, $currentContext.path).handle(event);
        }
        $currentContext = null;
    }

    function mutateRef(event: string) {
        if ($currentContext?.type == "Ref") {
            new RefMutator($currentContext.ref).handle(event);
        }
        $currentContext = null;
    }

    function requestInput(event: InputRequest) {
        $currentInput = Object.assign(event, {
            callback: (response: InputResponse) => {
                $currentInput = null;
                trigger("notify_input", { response });
            },
        });
    }
</script>

<Zone operand={{ type: "Repository" }} alwaysTarget let:target>
    <div
        id="shell"
        class={$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.theme_override : ""}>
        {#if $repoConfigEvent.type == "Initial"}
            <Pane>
                <h2 slot="header">Loading...</h2>
            </Pane>

            <div class="separator" />

            <Pane />
        {:else if $repoConfigEvent.type == "Workspace"}
            {#key $repoConfigEvent.absolute_path}
                <LogPane
                    default_query={$repoConfigEvent.default_query}
                    latest_query={$repoConfigEvent.latest_query} />
            {/key}

            <div class="separator" />

            <BoundQuery query={selection} let:data>
                {#if data.type == "Detail"}
                    <RevisionPane rev={data} />
                {:else}
                    <Pane>
                        <h2 slot="header">Not Found</h2>
                        <p slot="body">
                            Revision <IdSpan id={data.id.change} />|<IdSpan id={data.id.commit} /> does
                            not exist.
                        </p>
                    </Pane>
                {/if}
                <Pane slot="error" let:message>
                    <h2 slot="header">Error</h2>
                    <p slot="body">{message}</p>
                </Pane>
                <Pane slot="wait">
                    <h2 slot="header">Loading...</h2>
                </Pane>
            </BoundQuery>
        {:else if $repoConfigEvent.type == "LoadError"}
            <ModalOverlay>
                <ErrorDialog title="No Workspace Loaded">
                    <p>{$repoConfigEvent.message}.</p>
                    <p>Try opening a workspace from the Repository menu.</p>
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

        <div class="separator" style="grid-area: 2/1/3/4" />

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
                {#if $currentMutation.type == "data" && ($currentMutation.value.type == "InternalError" || $currentMutation.value.type == "PreconditionError")}
                    <ErrorDialog
                        title="Command Error"
                        onClose={() => ($currentMutation = null)}
                        severe>
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
    </div>
</Zone>

<style>
    #shell {
        width: 100vw;
        height: 100vh;

        display: grid;
        grid-template-columns: 1fr 3px 1fr;
        grid-template-rows: 1fr 3px 30px;
        grid-template-areas:
            "content content content"
            ". . ."
            "footer footer footer";

        background: var(--ctp-crust);
        color: var(--ctp-text);

        user-select: none;
    }

    .separator {
        background: var(--ctp-overlay0);
    }
</style>
