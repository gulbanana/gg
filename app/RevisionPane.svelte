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
    <div slot="header" class="metadata">
        {#if singleton}
            <span class="meta-item">
                <span class="meta-label">Change</span> <IdSpan selectable id={newest.id.change} />
            </span>
            <span class="meta-sep">&middot;</span>
            <span class="meta-item">
                <span class="meta-label">Commit</span> <IdSpan selectable id={newest.id.commit} />
            </span>
            <span class="meta-sep">&middot;</span>
            <span class="meta-item meta-inline">
                <AuthorSpan author={newest.author} />
            </span>
            <span class="meta-sep">&middot;</span>
            <span class="meta-item"><TimestampSpan timestamp={newest.author.timestamp} /></span>
            {#if newest.is_working_copy}
                <span class="meta-sep">&middot;</span>
                <span class="meta-item meta-flag">Working copy</span>
            {/if}
            {#if newest.is_immutable}
                <span class="meta-sep">&middot;</span>
                <span class="meta-item meta-flag">Immutable</span>
            {/if}
        {:else}
            <span class="meta-item">
                <SetSpan selectable set={revs.set} /> &middot; {revs.headers.length} revisions
            </span>
            <span class="meta-sep">&middot;</span>
            <span class="meta-item">
                {#each authors as author, ix}
                    <!-- prettier-ignore -->
                    <AuthorSpan {author} />{#if ix < authors.length - 1},&nbsp;{/if}
                {/each}
            </span>
            <span class="meta-sep">&middot;</span>
            <span class="meta-item"><TimestampRangeSpan from={firstTimestamp} to={lastTimestamp} /></span>
        {/if}
    </div>
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

        <div class="describe-commands">
            {#if singleton}
                <label class="reset-author-label">
                    <input type="checkbox" bind:checked={resetAuthor} disabled={newestImmutable} />
                    Reset author
                </label>
                <ActionWidget
                    tip="set commit message"
                    onClick={() => mutator.onDescribe(editableDescription, resetAuthor)}
                    disabled={newestImmutable || !descriptionChanged}>
                    Describe
                </ActionWidget>
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
            <div class="changes-header">
                <span>Changes ({syntheticChanges.length})</span>
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
                            <div class="change" style="--lines: {minLines(change)}" tabindex="-1">
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
            <div class="changes-header">
                <span>Changes: <span class="no-changes">(empty)</span></span>
            </div>
        {/if}
    </div>
</Pane>

<style>
    .body {
        height: 100%;
        overflow: hidden;
        display: flex;
        flex-direction: column;
        margin: 0 -6px -3px -6px;
        padding: 0 6px 3px 6px;
        gap: 0;
    }

    .metadata {
        display: flex;
        flex-wrap: wrap;
        align-items: baseline;
        gap: 0 6px;
        font-size: 13px;
        font-family: var(--gg-text-familyUi);
        line-height: 1.8;
        background: var(--gg-colors-background);
        margin: 0;
        padding: 3px;
    }

    .meta-item {
        pointer-events: auto;
        user-select: text;
        white-space: nowrap;
    }

    .meta-inline {
        display: inline-flex;
        align-items: center;
        gap: 4px;
    }

    .meta-label {
        color: var(--gg-colors-foregroundMuted);
    }

    .meta-sep {
        color: var(--gg-colors-outlineStrong);
    }

    .meta-flag {
        color: var(--gg-colors-foregroundMuted);
        font-style: italic;
    }

    .description {
        resize: vertical;
        min-height: 100px;
        overflow: auto;
        font-size: var(--gg-text-sizeMd);
    }

    .description-list {
        min-height: 100px;
        overflow: auto;
        pointer-events: auto;

        border: 1px solid transparent;
        border-radius: 4px;
        padding: 4px;

        white-space: pre-wrap;
        user-select: text;
        font-size: var(--gg-text-sizeMd);

        color: var(--gg-colors-foregroundMuted);
    }

    .description-row {
        white-space: pre-wrap;
    }

    .description-divider {
        border: none;
        border-top: 1px dashed var(--gg-colors-outline);
        margin: 4px 1px;
    }

    .describe-commands {
        display: flex;
        align-items: center;
        justify-content: end;
        gap: 6px;
        padding: 4px 0;
        flex-shrink: 0;
    }

    .reset-author-label {
        display: flex;
        align-items: center;
        gap: 4px;
        font-family: var(--gg-text-familyUi);
        font-size: 13px;
        color: var(--gg-colors-foregroundMuted);
        cursor: pointer;
        user-select: none;
    }

    .reset-author-label input[type="checkbox"]:disabled {
        cursor: default;
    }

    .reset-author-label:has(input:disabled) {
        cursor: default;
        opacity: 0.5;
    }

    .parents {
        border-top: 1px solid var(--gg-colors-outline);
        padding: 0 3px;
        font-size: 0.9em;
    }

    .parent {
        display: grid;
        grid-template-columns: 63px 1fr;
        align-items: baseline;
        gap: 6px;
    }

    .changes-header {
        border-top: 1px solid var(--gg-colors-outline);
        height: 30px;
        min-height: 30px;
        width: 100%;
        padding: 0 3px;
        display: flex;
        align-items: center;
        gap: 6px;
        color: var(--gg-colors-foregroundMuted);
        font-size: 13px;
    }

    .no-changes {
        color: var(--gg-colors-foregroundMuted);
    }

    .changes {
        border-top: 1px solid var(--gg-colors-outline);
        display: flex;
        flex-direction: column;
        pointer-events: auto;
        overflow-x: hidden;
        overflow-y: auto;
        scrollbar-color: var(--gg-colors-foreground) var(--gg-colors-surfaceDeep);
        flex: 1;
        min-height: 0;
    }

    .changes::-webkit-scrollbar {
        width: 6px;
    }

    .changes::-webkit-scrollbar-thumb {
        background-color: var(--gg-colors-foreground);
        border-radius: 6px;
    }

    .changes::-webkit-scrollbar-track {
        background-color: var(--gg-colors-surfaceDeep);
    }

    .change {
        font-size: small;
        margin: 0;
        pointer-events: auto;
        overflow-x: auto;
        overflow-y: scroll;
        scrollbar-color: var(--gg-colors-foreground) var(--gg-colors-background);
        min-height: calc(var(--lines) * 1em);
        border-bottom: var(--gg-components-borderSubtle);
    }

    .change::-webkit-scrollbar {
        width: 6px;
        height: 6px;
    }

    .change::-webkit-scrollbar-thumb {
        background-color: var(--gg-colors-foreground);
        border-radius: 6px;
    }

    .change::-webkit-scrollbar-track {
        background-color: var(--gg-colors-background);
    }

    .hunk {
        margin: 0;
        text-align: center;
        background: var(--gg-colors-surface);
    }

    .diff {
        margin: 0;
        background: var(--gg-colors-background);
        font-family: var(--gg-text-familyCode);
        font-size: var(--gg-text-sizeMd);
        user-select: text;
    }

    .add {
        color: var(--gg-colors-success);
    }

    .remove {
        color: var(--gg-colors-error);
    }

    .target {
        color: var(--gg-colors-primaryContent);
        background: var(--gg-colors-primary);
    }
</style>
