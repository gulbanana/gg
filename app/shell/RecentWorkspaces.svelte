<script lang="ts">
    import { query } from "../ipc.js";
    import { repoConfigEvent } from "../stores.js";
    import type { RepoConfig } from "../messages/RepoConfig.js";

    export let workspaces: string[] = [];

    async function openWorkspace(path: string) {
        const result = await query<RepoConfig>("query_workspace", { path });
        if (result.type === "data") {
            repoConfigEvent.set(result.value);
        } else {
            repoConfigEvent.set({
                type: "LoadError",
                absolute_path: path,
                message: result.message,
            });
        }
    }
</script>

{#if workspaces.length > 0}
    <h3>Recent Workspaces</h3>
    <ul>
        {#each workspaces as workspace}
            <li>
                <button on:click={() => openWorkspace(workspace)} title="open this workspace">
                    {workspace}
                </button>
            </li>
        {/each}
    </ul>
{/if}

<style>
    h3 {
        grid-column: 1/3;
        justify-self: center;
    }

    ul {
        margin-top: 9px;
        grid-column: 1/3;
        list-style-type: none;
    }

    li {
        margin: 9px 0;
    }

    button {
        background: none;
        border: none;
        color: var(--ctp-blue);
        cursor: pointer;
        text-decoration: none;
        font-family: var(--stack-code);
        font-size: 18px;
        padding: 0;

        &:hover {
            color: var(--ctp-sky);
            text-decoration: underline;
        }
    }
</style>
