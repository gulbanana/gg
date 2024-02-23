<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import type { RevId } from "./messages/RevId.js";
    import type { RevHeader } from "./messages/RevHeader.js";
    import type { RevDetail } from "./messages/RevDetail.js";
    import type { RepoConfig } from "./messages/RepoConfig.js";
    import type { RepoStatus } from "./messages/RepoStatus.js";
    import { command, event } from "./ipc.js";
    import Bound from "./Bound.svelte";
    import Icon from "./Icon.svelte";
    import Pane from "./Pane.svelte";
    import LogPane from "./LogPane.svelte";
    import RevisionPane from "./RevisionPane.svelte";

    const repo_config = event<RepoConfig>("gg://repo/config");
    const repo_status = event<RepoStatus>("gg://repo/status");
    const change_selection = event<RevHeader>("gg://change/select");
    const change_content = command<RevDetail>("get_revision");

    $: if ($repo_config) load_repo($repo_config);
    $: if ($change_selection) load_change($change_selection.commit_id);

    async function load_repo(config: RepoConfig) {
        change_content.reset();
        if (config.type == "Workspace") {
            $repo_status = config.status;
        }
    }

    async function load_change(id: RevId) {
        change_content.call({
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
    {#if $repo_config?.type == "Workspace"}
        {#key $repo_config.absolute_path}
            <LogPane query={$repo_config.default_revset} />
        {/key}
        <Bound query={$change_content} let:data>
            <RevisionPane rev={data} />
            <Pane slot="wait" />
            <Pane slot="error">
                <h2 slot="header">Error</h2>
            </Pane>
        </Bound>

        <div id="status-bar">
            <span>{$repo_config?.absolute_path}</span>
            <span />
            <span>{$repo_status?.operation_description}</span>
            <button><Icon name="rotate-ccw" /> Undo</button>
        </div>
    {:else if !$repo_config}
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
                {#if $repo_config.type == "NoWorkspace"}
                    <h2>No Workspace Loaded</h2>
                {:else if $repo_config.type == "NoOperation"}
                    <h2 class="error-text">Workspace Load Failed</h2>
                {:else}
                    <h2 class="error-text">Internal Error</h2>
                {/if}
                <p>{$repo_config.error}</p>
                <p>Try opening a workspace from the Repository menu.</p>
            </div>
        </div>
        <div id="status-bar">
            {#if $repo_config.type != "DeadWorker"}
                <span>{$repo_config?.absolute_path}</span>
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
