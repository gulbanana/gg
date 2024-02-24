<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import type { RevId } from "./messages/RevId.js";
    import type { RevDetail } from "./messages/RevDetail.js";
    import type { RepoConfig } from "./messages/RepoConfig.js";
    import { command } from "./ipc.js";
    import { repoConfig, repoStatus } from "./events.js";
    import { revisionSelect } from "./events.js";
    import Bound from "./Bound.svelte";
    import Icon from "./Icon.svelte";
    import Pane from "./Pane.svelte";
    import LogPane from "./LogPane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import Action from "./Action.svelte";

    const changeCommand = command<RevDetail>("get_revision");

    $: if ($repoConfig) load_repo($repoConfig);
    $: if ($revisionSelect) load_change($revisionSelect.commit_id);

    async function load_repo(config: RepoConfig) {
        changeCommand.reset();
        if (config.type == "Workspace") {
            $repoStatus = config.status;
        }
    }

    async function load_change(id: RevId) {
        changeCommand.call({
            rev: id.prefix + id.rest,
        });
    }

    document.addEventListener("keydown", (event) => {
        if (event.key === "o" && event.ctrlKey) {
            event.preventDefault();
            invoke("forward_accelerator", { key: "o" });
        }
    });

    invoke("notify_window_ready");
</script>

<div id="shell">
    {#if $repoConfig?.type == "Workspace"}
        {#key $repoConfig.absolute_path}
            <LogPane query={$repoConfig.default_revset} />
        {/key}
        <Bound query={$changeCommand} let:data>
            <RevisionPane rev={data} />
            <Pane slot="wait" />
            <Pane slot="error">
                <h2 slot="header">Error</h2>
            </Pane>
        </Bound>

        <div id="status-bar">
            <span>{$repoConfig?.absolute_path}</span>
            <span />
            <span>{$repoStatus?.operation_description}</span>
            <Action><Icon name="rotate-ccw" /> Undo</Action>
        </div>
    {:else if !$repoConfig}
        <div id="error-overlay">
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
        <div id="error-overlay">
            <div id="error-content">
                {#if $repoConfig.type == "NoWorkspace"}
                    <h2>No Workspace Loaded</h2>
                {:else if $repoConfig.type == "NoOperation"}
                    <h2 class="error-text">Workspace Load Failed</h2>
                {:else}
                    <h2 class="error-text">Internal Error</h2>
                {/if}
                <p>{$repoConfig.error}</p>
                <p>Try opening a workspace from the Repository menu.</p>
            </div>
        </div>
        <div id="status-bar">
            {#if $repoConfig.type != "DeadWorker"}
                <span>{$repoConfig?.absolute_path}</span>
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

    #error-overlay {
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
