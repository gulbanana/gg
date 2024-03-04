<script lang="ts">
    import type { RevDetail } from "./messages/RevDetail";
    import type { DescribeRevision } from "./messages/DescribeRevision";
    import type { CheckoutRevision } from "./messages/CheckoutRevision";
    import type { CreateRevision } from "./messages/CreateRevision";
    import { mutate } from "./ipc";
    import Action from "./Action.svelte";
    import Icon from "./Icon.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";
    import PathSpan from "./PathSpan.svelte";
    import RevisionSummary from "./RevisionSummary.svelte";
    export let rev: RevDetail;

    let fullDescription = rev.header.description.lines.join("\n");
    let selectedPath = "";

    function onDescribe() {
        mutate<DescribeRevision>("describe_revision", {
            change_id: rev.header.change_id,
            new_description: fullDescription,
        });
    }

    function onEdit() {
        mutate<CheckoutRevision>("checkout_revision", {
            change_id: rev.header.change_id,
        });
    }

    function onNew() {
        mutate<CreateRevision>("create_revision", {
            parent_change_ids: [rev.header.change_id],
        });
    }
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
        <textarea
            class="desc"
            spellcheck="false"
            disabled={rev.header.is_immutable}
            bind:value={fullDescription}
        />

        <div class="author">
            <span>{rev.header.author.name}</span>
            <span
                >{new Date(
                    rev.header.author.timestamp,
                ).toLocaleTimeString()}</span
            >
            <span></span>
            <Action onClick={onDescribe} disabled={rev.header.is_immutable}>
                <Icon name="file-text" /> Describe
            </Action>
        </div>

        <main>
            {#if rev.diff.length > 0}
                <section>
                    <h3>File changes</h3>
                    {#each rev.diff as path}
                        <button
                            class="unbutton path"
                            class:selected={selectedPath == path.relative_path}
                            on:click={() => (selectedPath = path.relative_path)}
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
                        <RevisionSummary rev={parent} selected={false} />
                    {/each}
                </section>
            {/if}
        </main>

        <div class="commands">
            <Action
                onClick={onEdit}
                disabled={rev.header.is_immutable || rev.header.is_working_copy}
            >
                <Icon name="edit-2" /> Edit
            </Action>
            <Action onClick={onNew}>
                <Icon name="edit" /> New
            </Action>
        </div>
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

    .commands {
        height: 30px;
        padding: 0 3px;
        display: flex;
        align-items: center;
        justify-content: end;
        gap: 6px;
    }

    section {
        background: var(--ctp-mantle);
        border-radius: 6px;
        padding: 3px;
        display: flex;
        flex-direction: column;
        margin-top: 3px;
        margin-bottom: 9px;
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
