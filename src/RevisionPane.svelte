<script lang="ts">
    import type { RevResult } from "./messages/RevResult";
    import { dragOverWidget, menuCommitEvent } from "./stores";
    import ChangeObject from "./objects/ChangeObject.svelte";
    import RevisionObject from "./objects/RevisionObject.svelte";
    import RevisionMutator from "./mutators/RevisionMutator";
    import ActionWidget from "./controls/ActionWidget.svelte";
    import Icon from "./controls/Icon.svelte";
    import IdSpan from "./controls/IdSpan.svelte";
    import Pane from "./Pane.svelte";
    import CheckWidget from "./controls/CheckWidget.svelte";
    import GraphNode from "./GraphNode.svelte";
    import Zone from "./objects/Zone.svelte";

    export let rev: Extract<RevResult, { type: "Detail" }>;

    let mutator = new RevisionMutator(rev.header);
    let fullDescription = rev.header.description.lines.join("\n");
    let resetAuthor = false;

    let unresolvedConflicts = rev.conflicts.filter(
        (conflict) =>
            rev.changes.findIndex((change) => !change.has_conflict && change.path.repo_path == conflict.repo_path) ==
            -1,
    );

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
            {#if rev.header.is_immutable}
                | Immutable
            {/if}
        </span>

        <div class="checkout-commands">
            <ActionWidget onClick={mutator.onNew}>
                <Icon name="edit" /> New
            </ActionWidget>
            <ActionWidget onClick={mutator.onEdit} disabled={rev.header.is_immutable || rev.header.is_working_copy}>
                <Icon name="edit-2" /> Edit
            </ActionWidget>
            <ActionWidget onClick={mutator.onDuplicate}>
                <Icon name="copy" /> Duplicate
            </ActionWidget>
        </div>
    </h2>

    <div slot="body" class="body">
        <textarea
            class="desc"
            spellcheck="false"
            disabled={rev.header.is_immutable}
            bind:value={fullDescription}
            on:dragenter={dragOverWidget}
            on:dragover={dragOverWidget} />

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

        <div class="objects">
            {#if rev.parents.length > 0}
                <Zone operand={{ type: "Merge", header: rev.header }} let:target>
                    <section class:target>
                        <h3>Parent revisions</h3>
                        {#each rev.parents as parent}
                            <div class="row">
                                <svg>
                                    <foreignObject x="0" y="0" width="100%" height="30">
                                        <RevisionObject header={parent} child={rev.header} selected={false} />
                                    </foreignObject>
                                    <GraphNode header={parent} />
                                </svg>
                            </div>
                        {/each}
                    </section>
                </Zone>
            {/if}

            {#if rev.changes.length > 0}
                <div class="move-commands">
                    <ActionWidget
                        onClick={mutator.onSquash}
                        disabled={rev.header.is_immutable || rev.header.parent_ids.length != 1}>
                        <Icon name="upload" /> Squash
                    </ActionWidget>
                    <ActionWidget
                        onClick={mutator.onRestore}
                        disabled={rev.header.is_immutable || rev.header.parent_ids.length != 1}>
                        <Icon name="download" /> Restore
                    </ActionWidget>
                </div>

                <section>
                    <h3>Changed files</h3>
                    {#each rev.changes as change}
                        <ChangeObject header={rev.header} {change} />
                    {/each}
                </section>
            {/if}

            {#if unresolvedConflicts.length > 0}
                <section class="conflict">
                    <h3>Unresolved conflicts</h3>
                    {#each unresolvedConflicts as conflict}
                        <div class="row">
                            {conflict.relative_path}
                        </div>
                    {/each}
                </section>
            {/if}
        </div>
    </div>
</Pane>

<style>
    .header {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        align-items: center;
        text-wrap: nowrap;
        font-weight: normal;
    }

    .title {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .checkout-commands {
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

    .objects {
        flex: 1;
        overflow: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-mantle);
    }

    section {
        background: var(--ctp-mantle);
        color: var(--ctp-text);
        border-radius: 6px;
        padding: 3px;
        display: flex;
        flex-direction: column;
        margin: 3px 0;
    }

    section > :global(*):not(:first-child) {
        height: 30px;
    }

    section.conflict {
        background: repeating-linear-gradient(
            120deg,
            var(--ctp-mantle) 0px,
            var(--ctp-mantle) 12px,
            var(--ctp-surface0) 12px,
            var(--ctp-surface0) 15px
        );
    }

    section.conflict > :global(*):not(:first-child) {
        margin-left: 24px;
    }

    section.target {
        color: black;
        background: var(--ctp-flamingo);
    }

    .move-commands {
        height: 30px;
        width: 100%;
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    h3 {
        font-size: 1rem;
        border-bottom: 1px solid var(--ctp-surface0);
    }

    .row {
        display: flex;
        align-items: center;
        --leftpad: 24px;
    }

    svg {
        width: 100%;
        height: 27px;
    }

    foreignObject {
        width: 100%;
        height: 30px;
        padding-right: 3px;
    }
</style>
