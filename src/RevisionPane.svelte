<script lang="ts">
    import Icon from "./Icon.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";
    import PathSpan from "./PathSpan.svelte";
    import type { RevDetail } from "./messages/RevDetail";

    export let rev: RevDetail;

    $: selected_path = "";
</script>

<Pane>
    <h2 slot="header" class="header">
        <span>
            <IdSpan type="change" id={rev.header.change_id} />
            /
            <IdSpan type="commit" id={rev.header.commit_id} />
        </span>
        <button><Icon name="map-pin" /> Pin</button>
    </h2>

    <div slot="body" class="body">
        <textarea class="desc" spellcheck="false"
            >{rev.header.description.lines.join("\n")}</textarea
        >

        <div class="author">
            <span>{rev.header.author}</span>
            <span>{new Date(rev.header.timestamp).toLocaleTimeString()}</span>
            <span></span>
            <button><Icon name="file-text" /> Describe</button>
        </div>

        <div class="diff">
            <h3>File changes</h3>
            {#each rev.diff as path}
                <!-- svelte-ignore a11y-click-events-have-key-events -->
                <!-- svelte-ignore a11y-no-static-element-interactions -->
                <div
                    class="path"
                    class:selected={selected_path == path.relative_path}
                    on:click={() => (selected_path = path.relative_path)}
                >
                    <PathSpan {path} />
                </div>
            {/each}
        </div>

        <div class="diff">
            <h3>Parents</h3>
            {#each rev.parents as parent}
                <div class="parent">
                    <code>
                        <IdSpan type="change" id={parent.change_id} />
                    </code>
                    <span>{parent.description.lines[0]}</span>
                    <code>
                        <IdSpan type="commit" id={parent.commit_id} />
                    </code>
                </div>
            {/each}
        </div>
    </div>
</Pane>

<style>
    .header {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }
    .header > button {
        background: var(--ctp-sapphire);
    }

    .body {
        display: flex;
        flex-direction: column;
        align-items: stretch;
        gap: 3px;
    }

    .desc {
        border-radius: 6px;
        width: 100%;
        height: 5em;
    }

    .author {
        color: var(--ctp-subtext0);
        width: 100%;
        display: grid;
        grid-template-columns: auto auto 1fr auto;
        gap: 6px;
    }
    .author > button {
        background: var(--ctp-peach);
    }

    .diff {
        background: var(--ctp-mantle);
        border-radius: 6px;
        padding: 3px;
        display: flex;
        flex-direction: column;
        margin-top: 9px;
    }
    .path {
        height: 24px;
        display: flex;
        align-items: center;
        cursor: pointer;
    }
    .selected {
        background: var(--ctp-base);
    }
    .parent {
        display: grid;
        grid-template-columns: auto 1fr auto;
        gap: 6px;
    }
    h3 {
        font-size: 1rem;
    }
</style>
