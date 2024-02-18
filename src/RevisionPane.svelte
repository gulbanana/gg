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
            <IdSpan id={rev.header.change_id} type="change" />
            /
            <IdSpan id={rev.header.commit_id} type="commit" />
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

        <div class="commands">
            <button>Abandon</button>
            <button>Squash</button>
            <button>Restore</button>
        </div>
    </div></Pane
>

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

    .commands {
        display: flex;
        justify-content: end;
        gap: 6px;
    }
    .commands > button {
        background: var(--ctp-maroon);
    }
</style>
