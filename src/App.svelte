<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import type { RevId } from "./messages/RevId.js";
    import type { RevDetail } from "./messages/RevDetail.js";
    import type { RepoConfig } from "./messages/RepoConfig.js";
    import { command } from "./ipc.js";
    import {
        currentMutation,
        repoConfigEvent,
        repoStatusEvent,
    } from "./stores.js";
    import { revisionSelectEvent } from "./stores.js";
    import Bound from "./Bound.svelte";
    import Icon from "./Icon.svelte";
    import Pane from "./Pane.svelte";
    import LogPane from "./LogPane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import Action from "./Action.svelte";

    const queryRevisionCommand = command<RevDetail>("query_revision");

    document.addEventListener("keydown", (event) => {
        if (event.key === "o" && event.ctrlKey) {
            event.preventDefault();
            invoke("forward_accelerator", { key: "o" });
        }
    });

    invoke("notify_window_ready");

    $: if ($repoConfigEvent) load_repo($repoConfigEvent);
    $: if ($repoStatusEvent && $revisionSelectEvent)
        load_change($revisionSelectEvent.change_id);

    async function load_repo(config: RepoConfig) {
        queryRevisionCommand.reset();
        if (config.type == "Workspace") {
            $repoStatusEvent = config.status;
        }
    }

    async function load_change(id: RevId) {
        queryRevisionCommand.query({
            rev: id.prefix + id.rest,
        });
    }

    function onUndo() {}
</script>

<div id="shell">
    {#if $repoConfigEvent?.type == "Workspace"}
        {#key $repoConfigEvent.absolute_path}
            <LogPane
                default_query={$repoConfigEvent.default_query}
                latest_query={$repoConfigEvent.latest_query}
            />
        {/key}
        <Bound query={$queryRevisionCommand} let:data>
            <RevisionPane rev={data} />
            <Pane slot="wait" />
            <Pane slot="error" let:message>
                <h2 slot="header">Error</h2>
                <p slot="body" class="error-text">{message}</p>
            </Pane>
        </Bound>

        <div id="status-bar">
            <span>{$repoConfigEvent?.absolute_path}</span>
            <span />
            <span>{$repoStatusEvent?.operation_description}</span>
            <Action onClick={onUndo}><Icon name="rotate-ccw" /> Undo</Action>
        </div>

        {#if $currentMutation}
            <div id="overlay">
                {#if $currentMutation.type == "data"}
                    {#if $currentMutation.value.type == "Failed"}
                        <div id="overlay-chrome">
                            <div id="overlay-content">
                                <h3 class="error-text">Command Error</h3>
                                <p>
                                    {$currentMutation.value.message}
                                </p>
                            </div>

                            <Action
                                safe
                                onClick={() => ($currentMutation = null)}
                                ><Icon name="x" /></Action
                            >
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

                        <Action safe onClick={() => ($currentMutation = null)}
                            ><Icon name="x" /></Action
                        >
                    </div>
                {/if}
            </div>
        {/if}
    {:else if !$repoConfigEvent}
        <div id="fatal-error">
            <div id="error-content">
                <p class="error-text">
                    Error communicating with backend. You'll need to restart GG
                    to continue.
                </p>
            </div>
        </div>
        <div id="status-bar">
            <span>Internal Error</span>
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
        <div id="status-bar">
            {#if $repoConfigEvent.type != "DeadWorker"}
                <span>{$repoConfigEvent?.absolute_path}</span>
            {:else}
                <span>Internal Error</span>
            {/if}
        </div>
    {/if}
</div>

<style>
    #shell {
        width: 100vw;
        height: 100vh;

        display: grid;
        grid-template-columns: 1fr 1fr;
        grid-template-rows: 1fr 26px;
        gap: 3px;

        background: var(--ctp-overlay0);
        color: var(--ctp-text);

        user-select: none;
    }

    #status-bar {
        grid-column: 1/3;
        padding: 0 3px;

        display: grid;
        grid-template-columns: auto 1fr auto auto;
        gap: 6px;
        align-items: center;

        background: var(--ctp-crust);
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
        grid-template-columns: 30px 1fr 30px;
        grid-template-rows: 30px auto 30px;
    }

    #overlay-chrome > :global(button) {
        grid-area: 1/3/1/3;
        height: 30px;
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
        grid-column: 1/3;
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
