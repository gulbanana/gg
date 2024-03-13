<script lang="ts">
    import type { RevId } from "./messages/RevId";
    import type { RevResult } from "./messages/RevResult";
    import type { RepoConfig } from "./messages/RepoConfig";
    import type { UndoOperation } from "./messages/UndoOperation";
    import type { Event } from "@tauri-apps/api/event";
    import { type Query, query, command, mutate, delay, onEvent } from "./ipc.js";
    import {
        currentMutation,
        currentContext,
        repoConfigEvent,
        repoStatusEvent,
        revisionSelectEvent,
    } from "./stores.js";
    import BranchMutator from "./mutators/BranchMutator";
    import ChangeMutator from "./mutators/ChangeMutator";
    import RevisionMutator from "./mutators/RevisionMutator";
    import Pane from "./Pane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import LogPane from "./LogPane.svelte";
    import BoundQuery from "./controls/BoundQuery.svelte";
    import Icon from "./controls/Icon.svelte";
    import ActionWidget from "./controls/ActionWidget.svelte";
    import Zone from "./objects/Zone.svelte";

    let selection: Query<RevResult> = {
        type: "wait",
    };

    document.addEventListener("keydown", (event) => {
        if (event.key === "o" && event.ctrlKey) {
            event.preventDefault();
            command("forward_accelerator", { key: "o" });
        } else if (event.key == "escape") {
            currentMutation.set(null);
        }
    });

    document.body.addEventListener("click", () => currentContext.set(null), true);

    command("notify_window_ready");

    onEvent("gg://context/revision", mutateRevision);
    onEvent("gg://context/tree", mutateTree);
    onEvent("gg://context/branch", mutateBranch);

    $: if ($repoConfigEvent) loadRepo($repoConfigEvent);
    $: if ($repoStatusEvent && $revisionSelectEvent) loadChange($revisionSelectEvent.change_id);

    async function loadRepo(config: RepoConfig) {
        $revisionSelectEvent = undefined;
        if (config.type == "Workspace") {
            $repoStatusEvent = config.status;
        }
    }

    async function loadChange(id: RevId) {
        let fetch = await query<RevResult>("query_revision", {
            query: id.hex,
        });

        let rev = await Promise.race([fetch, delay<RevResult>()]);

        if (rev.type == "wait") {
            selection = rev;
            rev = await fetch;
        }

        if (rev.type == "data" && rev.value.type == "NotFound" && id.hex != $repoStatusEvent?.working_copy.hex) {
            return loadChange($repoStatusEvent?.working_copy!);
        }

        selection = rev;
    }

    function mutateRevision(event: Event<string>) {
        console.log(`mutateRevision(${event.payload})`, $currentContext);
        if ($currentContext?.type == "Revision") {
            new RevisionMutator($currentContext.header).handle(event.payload);
        }
        $currentContext = null;
    }

    function mutateTree(event: Event<string>) {
        console.log(`mutateTree(${event.payload})`, $currentContext);
        if ($currentContext?.type == "Change") {
            new ChangeMutator($currentContext.header, $currentContext.path).handle(event.payload);
        }
        $currentContext = null;
    }

    function mutateBranch(event: Event<string>) {
        console.log(`mutateBranch(${event.payload})`, $currentContext);
        if ($currentContext?.type == "Branch") {
            new BranchMutator($currentContext.header, $currentContext.name).handle(event.payload);
        }
        $currentContext = null;
    }

    function onUndo() {
        mutate<UndoOperation>("undo_operation", null);
    }
</script>

<Zone operand={{ type: "Repository" }} let:target>
    <div id="shell" class:target class={$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.theme : ""}>
        {#if $repoConfigEvent?.type == "Workspace"}
            {#key $repoConfigEvent.absolute_path}
                <LogPane default_query={$repoConfigEvent.default_query} latest_query={$repoConfigEvent.latest_query} />
            {/key}

            <div class="separator" />

            <BoundQuery query={selection} let:data>
                {#if data.type == "Detail"}
                    <RevisionPane rev={data} />
                {:else}
                    <Pane>
                        <h2 slot="header">Not Found</h2>
                        <p slot="body">Revset '{data.query}' is empty.</p>
                    </Pane>
                {/if}
                <Pane slot="error" let:message>
                    <h2 slot="header">Error</h2>
                    <p slot="body" class="error-text">{message}</p>
                </Pane>
                <Pane slot="wait">
                    <h2 slot="header">Loading...</h2>
                </Pane>
            </BoundQuery>

            {#if $currentMutation}
                <div id="overlay">
                    {#if $currentMutation.type == "data"}
                        {#if $currentMutation.value.type == "InternalError" || $currentMutation.value.type == "PreconditionError"}
                            <div id="overlay-chrome">
                                <div id="overlay-content">
                                    <h3 class="error-text">Command Error</h3>
                                    <p>
                                        {$currentMutation.value.message}
                                    </p>
                                </div>

                                <ActionWidget safe onClick={() => ($currentMutation = null)}>
                                    <Icon name="x" />
                                </ActionWidget>
                            </div>
                        {/if}
                    {:else if $currentMutation.type == "error"}
                        <div id="overlay-chrome">
                            <div id="overlay-content">
                                <h3 class="error-text">IPC Error</h3>
                                <p>
                                    {$currentMutation.message}
                                </p>
                            </div>

                            <ActionWidget safe onClick={() => ($currentMutation = null)}>
                                <Icon name="x" />
                            </ActionWidget>
                        </div>
                    {/if}
                </div>
            {/if}
        {:else if !$repoConfigEvent}
            <div id="fatal-error">
                <div id="error-content">
                    <p class="error-text">Error communicating with backend. You may need to restart GG to continue.</p>
                </div>
            </div>
        {:else}
            <div id="fatal-error">
                <div id="error-content">
                    {#if $repoConfigEvent.type == "NoWorkspace"}
                        <h2>No Workspace Loaded</h2>
                    {:else}
                        <h2 class="error-text">Internal Error</h2>
                    {/if}
                    <p>{$repoConfigEvent.error}</p>
                    <p>Try opening a workspace from the Repository menu.</p>
                </div>
            </div>
        {/if}

        <div class="separator span" />

        <div id="status-bar" class="span">
            <span>{$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.absolute_path : "No workspace"}</span>
            <span id="status-operation">{$repoStatusEvent?.operation_description ?? "no operation"}</span>
            <ActionWidget onClick={onUndo} disabled={!$repoConfigEvent}>
                <Icon name="rotate-ccw" /> Undo
            </ActionWidget>
        </div>
    </div>
</Zone>

<style>
    #shell {
        width: 100vw;
        height: 100vh;

        display: grid;
        grid-template-columns: 1fr 3px 1fr;
        grid-template-rows: 1fr 3px 30px;

        background: var(--ctp-crust);
        color: var(--ctp-text);

        user-select: none;
    }

    #shell.target {
        background: var(--ctp-peach);
        color: black;
    }

    .separator {
        background: var(--ctp-overlay0);
    }

    .span {
        grid-column: 1/4;
    }

    #status-bar {
        padding: 0 9px;

        display: grid;
        grid-template-columns: auto 1fr auto;
        gap: 6px;
        align-items: center;
    }

    #status-operation {
        display: flex;
        justify-content: end;
        white-space: nowrap;
        overflow: hidden;
    }

    #overlay {
        z-index: 1;
        position: absolute;
        top: 0;
        right: 0;
        bottom: 0;
        left: 0;
        background: rgb(var(--ctp-overlay1-rgb) / 40%);

        display: grid;
        grid-template-columns: 1fr auto 1fr;
        grid-template-rows: 1fr auto 2fr;
    }

    #overlay-chrome {
        grid-area: 2/2/2/2;

        background: var(--ctp-mantle);
        border-radius: 9px;
        border: 3px solid var(--ctp-overlay1);

        display: grid;
        grid-template-columns: 30px 1fr 33px;
        grid-template-rows: 30px auto 30px;
    }

    #overlay-chrome > :global(button) {
        grid-area: 1/3/1/3;
        width: 30px;
        height: 30px;
        margin: 1px 3px 0 0;
    }

    #overlay-content {
        grid-area: 2/2/2/2;
        padding: 0 30px;
    }

    #overlay-content > :first-child {
        margin-top: 0;
    }

    #overlay-content > :last-child {
        margin-bottom: 0;
    }

    #fatal-error {
        grid-column: 1/4;
        display: grid;
        align-items: center;
        justify-content: center;
    }

    #error-content {
        background: var(--ctp-mantle);
        padding: 30px;
        border-radius: 9px;
    }

    #error-content > p:last-child {
        margin-bottom: 0;
    }

    .error-text {
        color: var(--ctp-red);
    }
</style>
