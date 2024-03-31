<script lang="ts">
    import type { RevResult } from "./messages/RevResult";
    import { changeSelectEvent, dragOverWidget } from "./stores";
    import ChangeObject from "./objects/ChangeObject.svelte";
    import RevisionObject from "./objects/RevisionObject.svelte";
    import RevisionMutator from "./mutators/RevisionMutator";
    import ActionWidget from "./controls/ActionWidget.svelte";
    import Icon from "./controls/Icon.svelte";
    import IdSpan from "./controls/IdSpan.svelte";
    import Pane from "./shell/Pane.svelte";
    import CheckWidget from "./controls/CheckWidget.svelte";
    import Zone from "./objects/Zone.svelte";
    import { onEvent } from "./ipc";
    import AuthorSpan from "./controls/AuthorSpan.svelte";
    import ListWidget, { type List } from "./controls/ListWidget.svelte";

    export let rev: Extract<RevResult, { type: "Detail" }>;

    let mutator = new RevisionMutator(rev.header);
    let fullDescription = rev.header.description.lines.join("\n");
    let resetAuthor = false;

    let unresolvedConflicts = rev.conflicts.filter(
        (conflict) =>
            rev.changes.findIndex(
                (change) => !change.has_conflict && change.path.repo_path == conflict.path.repo_path,
            ) == -1,
    );

    let syntheticChanges = rev.changes
        .concat(
            unresolvedConflicts.map((conflict) => ({
                kind: "None",
                path: conflict.path,
                has_conflict: true,
                hunks: [conflict.hunk],
            })),
        )
        .sort((a, b) => a.path.relative_path.localeCompare(b.path.relative_path));

    let unset = true;
    let selectedChange = $changeSelectEvent;
    for (let change of syntheticChanges) {
        if (selectedChange?.path?.repo_path === change.path.repo_path) {
            unset = false;
        }
    }
    if (unset) {
        changeSelectEvent.set(syntheticChanges[0]);
    }

    let list: List = {
        getSize() {
            return syntheticChanges.length;
        },
        getSelection() {
            return syntheticChanges.findIndex((row) => row.path.repo_path == $changeSelectEvent?.path.repo_path) ?? -1;
        },
        selectRow(row: number) {
            $changeSelectEvent = syntheticChanges[row];
        },
        editRow(row: number) {},
    };

    onEvent<string>("gg://menu/revision", (event) => mutator.handle(event));
</script>

<Pane>
    <h2 slot="header" class="header">
        <span class="title">
            <IdSpan id={rev.header.id.change} /> | <IdSpan id={rev.header.id.commit} />
            {#if rev.header.is_working_copy}
                | Working copy
            {/if}
            {#if rev.header.is_immutable}
                | Immutable
            {/if}
        </span>

        <div class="checkout-commands">
            <ActionWidget
                tip="make working copy"
                onClick={mutator.onEdit}
                disabled={rev.header.is_immutable || rev.header.is_working_copy}>
                <Icon name="edit-2" /> Edit
            </ActionWidget>
            <ActionWidget tip="create a child" onClick={mutator.onNew}>
                <Icon name="edit" /> New
            </ActionWidget>
        </div>
    </h2>

    <div slot="body" class="body">
        <textarea
            class="description"
            spellcheck="false"
            disabled={rev.header.is_immutable}
            bind:value={fullDescription}
            on:dragenter={dragOverWidget}
            on:dragover={dragOverWidget} />

        <div class="signature-commands">
            <span>Author:</span>
            <AuthorSpan author={rev.header.author} includeTimestamp />
            <CheckWidget bind:checked={resetAuthor}>Reset</CheckWidget>
            <span></span>
            <ActionWidget
                tip="set commit message"
                onClick={() => mutator.onDescribe(fullDescription, resetAuthor)}
                disabled={rev.header.is_immutable}>
                <Icon name="file-text" /> Describe
            </ActionWidget>
        </div>

        {#if rev.parents.length > 0}
            <Zone operand={{ type: "Merge", header: rev.header }} let:target>
                <div class="parents" class:target>
                    {#each rev.parents as parent}
                        <div class="parent">
                            <span>Parent:</span>
                            <RevisionObject header={parent} child={rev.header} selected={false} noBranches />
                        </div>
                    {/each}
                </div>
            </Zone>
        {/if}

        {#if syntheticChanges.length > 0}
            <div class="move-commands">
                <span>Changes:</span>
                <ActionWidget
                    tip="move all changes to parent"
                    onClick={mutator.onSquash}
                    disabled={rev.header.is_immutable || rev.header.parent_ids.length != 1}>
                    <Icon name="upload" /> Squash
                </ActionWidget>
                <ActionWidget
                    tip="copy all changes from parent"
                    onClick={mutator.onRestore}
                    disabled={rev.header.is_immutable || rev.header.parent_ids.length != 1}>
                    <Icon name="download" /> Restore
                </ActionWidget>
            </div>

            <ListWidget {list} type="Change" descendant={$changeSelectEvent?.path.repo_path}>
                <div class="changes">
                    {#each syntheticChanges as change}
                        <ChangeObject
                            {change}
                            header={rev.header}
                            selected={$changeSelectEvent?.path?.repo_path === change.path.repo_path} />
                        {#if $changeSelectEvent?.path?.repo_path === change.path.repo_path}
                            <pre
                                class="change"
                                style="--lines: {Math.min(
                                    change.hunks[0].lines.length,
                                    6,
                                )}">{#each change.hunks[0].lines as line, ix}{line}{#if ix != change.hunks[0].lines.length - 1}<br />{/if}{/each}</pre>
                        {/if}
                    {/each}
                </div>
            </ListWidget>
        {:else}
            <div class="move-commands">
                <span>Changes: <span class="no-changes">(empty)</span></span>
            </div>
        {/if}
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
        height: 100%;
        overflow: hidden;
        display: grid;
        grid-template-rows: 90px 30px auto 30px 1fr;
        margin: 0 -6px -3px -6px;
        padding: 0 6px 3px 6px;
    }

    .signature-commands {
        height: 30px;
        width: 100%;
        display: grid;
        grid-template-columns: 63px auto auto 1fr auto;
        align-items: center;
        gap: 6px;
        padding: 0 3px;
    }

    .parents {
        border-top: 1px solid var(--ctp-overlay0);
        padding: 0 3px;
    }

    .parent {
        display: grid;
        grid-template-columns: 63px 1fr;
        align-items: baseline;
        gap: 6px;
    }

    .move-commands {
        border-top: 1px solid var(--ctp-overlay0);
        height: 30px;
        width: 100%;
        padding: 0 3px;
        display: grid;
        grid-template-columns: 1fr auto auto;
        align-items: center;
        gap: 6px;
    }

    .move-commands > :global(button) {
        margin-top: -1px;
    }

    .no-changes {
        color: var(--ctp-subtext0);
    }

    .changes {
        border-top: 1px solid var(--ctp-overlay0);
        display: flex;
        flex-direction: column;
        pointer-events: auto;
        overflow-x: hidden;
        overflow-y: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);
    }

    .change {
        font-family: var(--stack-code);
        font-size: small;
        min-height: calc(var(--lines) * 1em);
        margin: 0;
        padding: 0 3px;
        user-select: text;
        pointer-events: auto;
        overflow-x: auto;
        overflow-y: scroll;
        background: var(--ctp-base);
        scrollbar-color: var(--ctp-text) var(--ctp-base);
    }

    .target {
        color: black;
        background: var(--ctp-flamingo);
    }
</style>
