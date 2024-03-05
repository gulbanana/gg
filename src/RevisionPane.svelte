<script lang="ts">
    import type { RevDetail } from "./messages/RevDetail";
    import { menuCommitEvent } from "./stores";
    import ActionWidget from "./ActionWidget.svelte";
    import Icon from "./Icon.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";
    import PathSpan from "./PathSpan.svelte";
    import RevisionSummary from "./RevisionSummary.svelte";
    import CheckWidget from "./CheckWidget.svelte";
    import Mutator from "./Mutator";

    export let rev: RevDetail;

    let mutator = new Mutator(rev.header);
    let fullDescription = rev.header.description.lines.join("\n");
    let resetAuthor = false;
    let selectedPath = "";

    $: mutator.handle($menuCommitEvent);
</script>

<Pane>
    <h2 slot="header" class="header">
        <span class="title">
            <IdSpan type="change" id={rev.header.change_id} />
            | <IdSpan type="commit" id={rev.header.commit_id} />
            {#if rev.header.is_working_copy}
                | Working copy
            {/if}
        </span>

        <div class="primary-commands">
            <ActionWidget onClick={mutator.onNew}>
                <Icon name="edit" /> New
            </ActionWidget>
            <ActionWidget
                onClick={mutator.onEdit}
                disabled={rev.header.is_immutable ||
                    rev.header.is_working_copy}>
                <Icon name="edit-2" /> Edit
            </ActionWidget>
            <ActionWidget onClick={mutator.onDuplicate}>
                <Icon name="copy" /> Duplicate
            </ActionWidget>
            <ActionWidget
                onClick={mutator.onAbandon}
                disabled={rev.header.is_immutable}>
                <Icon name="trash-2" /> Abandon
            </ActionWidget>
        </div>
    </h2>

    <div slot="body" class="body">
        <textarea
            class="desc"
            spellcheck="false"
            disabled={rev.header.is_immutable}
            bind:value={fullDescription} />

        <div class="signature-commands">
            <span>
                {rev.header.author.name},
                {new Date(rev.header.author.timestamp).toLocaleTimeString()}
            </span>
            <CheckWidget bind:checked={resetAuthor}>Reset</CheckWidget>
            <span></span>
            <ActionWidget
                onClick={() => mutator.onDescribe(fullDescription, resetAuthor)}
                disabled={rev.header.is_immutable}>
                <Icon name="file-text" /> Describe
            </ActionWidget>
        </div>

        <main>
            {#if rev.diff.length > 0}
                <section>
                    <h3>File changes</h3>
                    {#each rev.diff as path}
                        <button
                            class="unbutton path"
                            class:selected={selectedPath == path.relative_path}
                            on:click={() =>
                                (selectedPath = path.relative_path)}>
                            <PathSpan {path} />
                        </button>
                    {/each}
                </section>

                <div class="move-commands">
                    <ActionWidget
                        onClick={mutator.onSquash}
                        disabled={rev.header.is_immutable ||
                            rev.header.parent_ids.length != 1}>
                        <Icon name="download" /> Squash
                    </ActionWidget>
                    <ActionWidget
                        onClick={mutator.onRestore}
                        disabled={rev.header.is_immutable ||
                            rev.header.parent_ids.length != 1}>
                        <Icon name="upload" /> Restore
                    </ActionWidget>
                </div>
            {/if}

            {#if rev.parents.length > 0}
                <section>
                    <h3>Parents</h3>
                    {#each rev.parents as parent}
                        <RevisionSummary rev={parent} selected={false} />
                    {/each}
                </section>
            {/if}
        </main>
    </div>
</Pane>

<style>
    .header {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        align-items: center;
        text-wrap: nowrap;
    }

    .title {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .primary-commands {
        height: 30px;
        padding: 0 3px;
        display: flex;
        align-items: center;
        justify-content: end;
        gap: 6px;
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

    .signature-commands {
        height: 30px;
        width: 100%;
        display: grid;
        grid-template-columns: auto auto 1fr auto;
        align-items: center;
        gap: 6px;
        padding-right: 3px;
        color: var(--ctp-subtext0);
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
        margin: 3px 0;
    }

    .move-commands {
        height: 30px;
        width: 100%;
        display: flex;
        align-items: center;
        justify-content: space-between;
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
