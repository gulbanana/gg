<script lang="ts">
    import { trigger } from "../ipc.js";

    let { workspaces = [] }: { workspaces?: string[] } = $props();
</script>

{#if workspaces.length > 0}
    <h3>Recent Workspaces</h3>
    <ul>
        {#each workspaces as workspace}
            <li>
                <button
                    onclick={() =>
                        trigger("open_workspace_at_path", {
                            path: workspace,
                        })}
                    title="open this workspace">
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
