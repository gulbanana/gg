<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import { revisionSelect } from "./events.js";
    import IdSpan from "./IdSpan.svelte";

    export let revision: RevHeader;
    export let selected: boolean; // same as the imported event, but parent may want to force a value
</script>

<button
    class="unbutton layout"
    class:selected
    class:conflict={revision.has_conflict}
    on:click={() => ($revisionSelect = revision)}
>
    <IdSpan type="change" id={revision.change_id} />

    <span
        class="desc"
        class:indescribable={revision.description.lines[0] == ""}
    >
        {revision.description.lines[0] == ""
            ? "(no description set)"
            : revision.description.lines[0]}
    </span>

    {#each revision.branches as ref}
        <code class="tag" class:conflict={ref.has_conflict}>
            {ref.remote == null ? ref.name : `${ref.name}@${ref.remote}`}
        </code>
    {/each}
</button>

<style>
    .layout {
        /* layout summary components along a text line */
        height: 100%;
        width: 100%;
        display: flex;
        align-items: baseline;
        gap: 6px;

        /* skip past svg lines when used in a graph */
        padding-left: var(--leftpad);
    }

    .layout :global(span) {
        line-height: 30px;
    }

    .layout.selected {
        background: var(--ctp-base);
    }

    .desc {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        flex: 1;
    }

    .desc.indescribable {
        color: var(--ctp-subtext0);
    }

    .tag {
        height: 24px;
        display: flex;
        align-items: center;
        border: 1px solid var(--ctp-overlay1);
        border-radius: 12px;
        padding: 0 6px;
        background: var(--ctp-crust);
        white-space: nowrap;
    }

    /* both nodes and refs can have this */
    .conflict {
        background: repeating-linear-gradient(
            120deg,
            transparent 0px,
            transparent 12px,
            var(--ctp-surface0) 12px,
            var(--ctp-surface0) 15px
        );
    }

    .selected.conflict {
        background: repeating-linear-gradient(
            120deg,
            var(--ctp-surface0) 0px,
            var(--ctp-surface0) 12px,
            var(--ctp-base) 12px,
            var(--ctp-base) 15px
        );
    }
</style>
