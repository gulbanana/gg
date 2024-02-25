<script lang="ts">
    import type { RevDetail } from "./messages/RevDetail";
    import Action from "./Action.svelte";
    import Icon from "./Icon.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";
    import PathSpan from "./PathSpan.svelte";
    import RevisionSummary from "./RevisionSummary.svelte";

    export let rev: RevDetail;

    let selected_path = "";
</script>

<Pane>
    <h2 slot="header" class="header">
        <span>
            <IdSpan type="change" id={rev.header.change_id} />
            | <IdSpan type="commit" id={rev.header.commit_id} />
            {#if rev.header.is_working_copy}
                | Working copy
            {/if}
        </span>
    </h2>

    <div slot="body" class="body">
        <textarea class="desc" spellcheck="false"
            >{rev.header.description.lines.join("\n")}</textarea
        >

        <div class="author">
            <span>{rev.author}</span>
            <span>{new Date(rev.timestamp).toLocaleTimeString()}</span>
            <span></span>
            <Action><Icon name="file-text" /> Describe</Action>
        </div>

        <main>
            {#if rev.diff.length > 0}
                <section>
                    <h3>File changes</h3>
                    {#each rev.diff as path}
                        <button
                            class="unbutton path"
                            class:selected={selected_path == path.relative_path}
                            on:click={() =>
                                (selected_path = path.relative_path)}
                        >
                            <PathSpan {path} />
                        </button>
                    {/each}
                </section>
            {/if}

            {#if rev.parents.length > 0}
                <section>
                    <h3>Parents</h3>
                    {#each rev.parents as parent}
                        <RevisionSummary revision={parent} selected={false} />
                    {/each}
                </section>
            {/if}
        </main>
    </div>
</Pane>

<style>
    .header {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .body {
        display: flex;
        flex-direction: column;
        align-items: stretch;
        gap: 3px;
        overflow: hidden;
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

    main {
        flex: 1;
        overflow: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-mantle);
    }

    section {
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

    h3 {
        font-size: 1rem;
    }
</style>
