<script lang="ts">
    import type { RevsResult } from "./messages/RevsResult";
    import { ignoreToggled, changeSelectEvent, dragOverWidget } from "./stores";
    import ChangeObject from "./objects/ChangeObject.svelte";
    import HunkObject from "./objects/HunkObject.svelte";
    import RevisionObject from "./objects/RevisionObject.svelte";
    import RevisionMutator from "./mutators/RevisionMutator";
    import ActionWidget from "./controls/ActionWidget.svelte";
    import Icon from "./controls/Icon.svelte";
    import IdSpan from "./controls/IdSpan.svelte";
    import Pane from "./shell/Pane.svelte";
    import ToggleWidget from "./controls/ToggleWidget.svelte";
    import Zone from "./objects/Zone.svelte";
    import { onEvent } from "./ipc";
    import AuthorSpan from "./controls/AuthorSpan.svelte";
    import ListWidget, { type List } from "./controls/ListWidget.svelte";
    import SetSpan from "./controls/SetSpan.svelte";
    import type { RevChange } from "./messages/RevChange";
    import TimestampSpan from "./controls/TimestampSpan.svelte";
    import TimestampRangeSpan from "./controls/TimestampRangeSpan.svelte";

    export let revs: Extract<RevsResult, { type: "Detail" }>;

    const CONTEXT = 3;

    // headers are in descendant-first order
    $: singleton = revs.set.from.commit.hex == revs.set.to.commit.hex;
    $: newest = revs.headers[0];
    $: oldest = revs.headers[revs.headers.length - 1];
    $: newestImmutable = newest.is_immutable && !$ignoreToggled;
    $: oldestImmutable = oldest.is_immutable && !$ignoreToggled;

    $: mutator = new RevisionMutator(revs.headers, $ignoreToggled);

    // debounce for change detection
    let lastSelectionKey = `${revs.set.from.commit.hex}::${revs.set.to.commit.hex}`;
    $: selectionKey = `${revs.set.from.commit.hex}::${revs.set.to.commit.hex}`;

    // editable description for single-revision mode
    let originalDescription = revs.headers[revs.headers.length - 1].description.lines.join("\n");
    $: editableDescription = revs.headers[revs.headers.length - 1].description.lines.join("\n");
    $: {
        if (selectionKey !== lastSelectionKey) {
            lastSelectionKey = selectionKey;
            originalDescription = editableDescription;
        }
    }
    $: descriptionChanged = originalDescription !== editableDescription;
    let resetAuthor = false;
    function updateDescription() {
        mutator.onDescribe(editableDescription, resetAuthor);
    }

    // grouped authors for range mode
    $: firstTimestamp = new Date(
        Math.min(...revs.headers.map((h) => new Date(h.author.timestamp).getTime())),
    ).toISOString();
    $: lastTimestamp = new Date(
        Math.max(...revs.headers.map((h) => new Date(h.author.timestamp).getTime())),
    ).toISOString();
    $: authors = [...new Map(revs.headers.map((h) => [h.author.email, h.author])).values()];

    let syntheticChanges = revs.changes
        .concat(
            revs.conflicts.map((conflict) => ({
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
            let index =
                syntheticChanges.findIndex((row) => row.path.repo_path == $changeSelectEvent?.path.repo_path) ?? -1;
            return { from: index, to: index };
        },
        selectRow(row: number) {
            $changeSelectEvent = syntheticChanges[row];
        },
        extendSelection(row: number) {
            $changeSelectEvent = syntheticChanges[row];
        },
        editRow(row: number) {},
    };

    onEvent<string>("gg://menu/revision", (event) => mutator.handle(event));

    function minLines(change: RevChange): number {
        // let total = 0;
        // for (let hunk of change.hunks) {
        //     total += Math.min(hunk.lines.lines.length, CONTEXT * 2 + 1) + 1;
        // }
        // return total;
        let max = 0;
        for (let hunk of change.hunks) {
            max = Math.max(hunk.lines.lines.length, max);
        }
        return Math.min(max, CONTEXT * 2 + 1);
    }

    function lineColour(line: string): string | null {
        if (line.startsWith("+")) {
            return "add";
        } else if (line.startsWith("-")) {
            return "remove";
        } else {
            return null;
        }
    }
</script>

<Pane>
    <h2 slot="header" class="header">
        <span class="title">
            {#if singleton}
                <IdSpan selectable id={newest.id.change} /> | <IdSpan selectable id={newest.id.commit} />
                {#if newest.is_working_copy}
                    | Working copy
                {/if}
            {:else}
                <SetSpan selectable set={revs.set} /> | {revs.headers.length} revisions
            {/if}
            {#if revs.headers.some((header) => header.is_immutable)}
                | Immutable
            {/if}
        </span>

        <div class="checkout-commands">
            {#if singleton}
                <ActionWidget
                    tip="make working copy"
                    onClick={mutator.onEdit}
                    disabled={newestImmutable || newest.is_working_copy}>
                    <Icon name="edit-2" /> Edit
                </ActionWidget>
            {/if}

            <ActionWidget tip="create a child" onClick={mutator.onNewChild}>
                <Icon name="edit" /> New
            </ActionWidget>
        </div>
    </h2>

    <div slot="body" class="body">
        {#if !singleton}
            <!-- prettier-ignore -->
            <div class="description-list">{#each revs.headers as header, i}{#if i > 0}<hr class="description-divider" />{/if}<div class="description-row">{header.description.lines.join("\n")}</div>{/each}</div>
        {:else}
            <textarea
                class="description"
                spellcheck="false"
                disabled={newestImmutable}
                bind:value={editableDescription}
                on:dragenter={dragOverWidget}
                on:dragover={dragOverWidget}
                on:keydown={(ev) => {
                    if (descriptionChanged && ev.key === "Enter" && (ev.metaKey || ev.ctrlKey)) {
                        updateDescription();
                    }
                }}></textarea>
        {/if}

        <div class="signature-commands">
            {#if singleton}
                <span>Author:</span>
                <AuthorSpan author={newest.author} />
                <TimestampSpan timestamp={newest.author.timestamp} />

                <ToggleWidget
                    safe
                    tip="reset author"
                    bind:checked={resetAuthor}
                    disabled={newest.is_immutable}
                    on="unlock"
                    off="lock" />
                <span></span>
                <ActionWidget
                    tip="set commit message"
                    onClick={() => mutator.onDescribe(editableDescription, resetAuthor)}
                    disabled={newestImmutable || !descriptionChanged}>
                    <Icon name="file-text" /> Describe
                </ActionWidget>
            {:else}
                {#if authors.length > 1}
                    <span>Authors:</span>
                {:else}
                    <span>Author:</span>
                {/if}
                <span>
                    {#each authors as author, ix}
                        <!-- prettier-ignore -->
                        <AuthorSpan {author} />{#if ix < authors.length - 1},&nbsp;
                        {/if}
                    {/each}
                </span>
                <TimestampRangeSpan from={firstTimestamp} to={lastTimestamp} />
            {/if}
        </div>

        {#if revs.parents.length > 0}
            <Zone operand={{ type: "Merge", header: oldest }} let:target>
                <div class="parents" class:target>
                    {#each revs.parents as parent}
                        <div class="parent">
                            <span>Parent:</span>
                            <RevisionObject header={parent} child={oldest} selected={false} noBookmarks />
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
                    disabled={oldestImmutable || oldest.parent_ids.length != 1}>
                    <Icon name="upload" /> Squash
                </ActionWidget>

                {#if singleton}
                    <ActionWidget
                        tip="copy all changes from parent"
                        onClick={mutator.onRestore}
                        disabled={newestImmutable || newest.parent_ids.length != 1}>
                        <Icon name="download" /> Restore
                    </ActionWidget>
                {/if}
            </div>

            <ListWidget {list} type="Change" descendant={$changeSelectEvent?.path.repo_path}>
                <div class="changes">
                    {#each syntheticChanges as change}
                        <!-- XXX implement, somehow, plural squash/restore -->
                        <ChangeObject
                            {change}
                            headers={revs.headers}
                            selected={$changeSelectEvent?.path?.repo_path === change.path.repo_path} />
                        {#if $changeSelectEvent?.path?.repo_path === change.path.repo_path}
                            <div class="change" style="--lines: {minLines(change)}">
                                {#each change.hunks as hunk}
                                    <div class="hunk">
                                        <HunkObject header={singleton ? newest : null} path={change.path} {hunk} />
                                    </div>
                                    <pre class="diff">{#each hunk.lines.lines as line}<span class={lineColour(line)}
                                                >{line}</span
                                            >{/each}</pre>
                                {/each}
                            </div>
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
        display: flex;
        flex-direction: column;
        margin: 0 -6px -3px -6px;
        padding: 0 6px 3px 6px;
        gap: 0;
    }

    .description {
        resize: vertical;
        min-height: 90px;
        overflow: auto;
    }

    .description-list {
        min-height: 90px;
        overflow: auto;
        pointer-events: auto;

        border: 1px solid transparent;
        border-radius: 4px;
        padding: 1px;

        white-space: pre-wrap;
        user-select: text;

        color: var(--ctp-subtext0);
    }

    .description-row {
        white-space: pre-wrap;
    }

    .description-divider {
        border: none;
        border-top: 1px dashed var(--ctp-overlay0);
        margin: 4px 1px;
    }

    .signature-commands {
        height: 30px;
        width: 100%;
        display: grid;
        grid-template-columns: 63px auto auto auto 1fr auto;
        align-items: center;
        gap: 6px;
        padding: 0 3px;
        flex-shrink: 0;
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
        min-height: 30px;
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
        flex: 1;
        min-height: 0;
    }

    .changes::-webkit-scrollbar {
        width: 6px;
    }

    .changes::-webkit-scrollbar-thumb {
        background-color: var(--ctp-text);
        border-radius: 6px;
    }

    .changes::-webkit-scrollbar-track {
        background-color: var(--ctp-crust);
    }

    .change {
        font-size: small;
        margin: 0;
        pointer-events: auto;
        overflow-x: auto;
        overflow-y: scroll;
        scrollbar-color: var(--ctp-text) var(--ctp-base);
        min-height: calc(var(--lines) * 1em);
    }

    .change::-webkit-scrollbar {
        width: 6px;
        height: 6px;
    }

    .change::-webkit-scrollbar-thumb {
        background-color: var(--ctp-text);
        border-radius: 6px;
    }

    .change::-webkit-scrollbar-track {
        background-color: var(--ctp-base);
    }

    .hunk {
        margin: 0;
        text-align: center;
        background: var(--ctp-mantle);
    }

    .diff {
        margin: 0;
        background: var(--ctp-base);
        user-select: text;
    }

    .add {
        color: var(--ctp-green);
    }

    .remove {
        color: var(--ctp-red);
    }

    .target {
        color: black;
        background: var(--ctp-flamingo);
    }
</style>
