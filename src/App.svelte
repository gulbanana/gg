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
        $repo_status = config.status;
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
    {#if $repo_config}
        {#key $repo_config.absolute_path}
            <LogPane query={$repo_config.default_revset} />
        {/key}
    {:else}
        <Pane />
    {/if}

    <Bound query={$change_content} let:data>
        <RevisionPane rev={data} />
        <Pane slot="wait" />
    </Bound>

    <div id="status-bar">
        <span>{$repo_config?.absolute_path}</span>
        <span />
        <span>{$repo_status?.operation_description}</span>
        <button><Icon name="rotate-ccw" /> Undo</button>
    </div>
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
</style>
