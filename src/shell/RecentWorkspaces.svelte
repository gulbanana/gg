<script lang="ts">
    import { trigger } from "../ipc.js";

    export let workspaces: string[] = [];
</script>

{#if workspaces.length > 0}
    <div class="recent-workspaces">
        <p><strong>recent workspaces:</strong></p>
        <ul class="workspaces-list">
            {#each workspaces as workspace}
                <li class="workspace-item">
                    <button
                        class="workspace-button"
                        on:click={() =>
                            trigger("open_workspace_at_path", {
                                path: workspace,
                            })}
                        title="open this workspace">
                        {workspace}
                    </button>
                </li>
            {/each}
        </ul>
    </div>
{/if}

<style>
    .recent-workspaces {
        margin-top: 16px;
    }

    .workspaces-list {
        text-align: left;
        margin: 8px 0;
        padding-left: 20px;
    }

    .workspace-item {
        margin: 10px 0;
        font-family: monospace;
        font-size: 12px;
        color: var(--ctp-subtext1);
    }

    .workspace-button {
        background: none;
        border: none;
        color: var(--ctp-blue);
        cursor: pointer;
        text-decoration: none;
        font-family: monospace;
        font-size: 18px;
        padding: 0;
        text-align: left;
    }

    .workspace-button:hover {
        color: var(--ctp-sky);
        text-decoration: underline;
    }
</style>
