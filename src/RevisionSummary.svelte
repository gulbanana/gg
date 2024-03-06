<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import type { CheckoutRevision } from "./messages/CheckoutRevision";
    import type { CreateRevision } from "./messages/CreateRevision";
    import { invoke } from "@tauri-apps/api/core";
    import { revisionSelectEvent, currentContext } from "./stores.js";
    import { mutate } from "./ipc";
    import IdSpan from "./IdSpan.svelte";

    export let rev: RevHeader;
    export let selected: boolean; // same as the imported event, but parent may want to force a value

    function onSelect() {
        revisionSelectEvent.set(rev);
    }

    function onMenu(event: Event) {
        event.preventDefault();

        currentContext.set(rev);

        // XXX no error handling
        invoke("forward_context_menu", {
            context: rev,
        });
    }

    function onEdit() {
        if (rev.is_working_copy) {
            return;
        }

        if (rev.is_immutable) {
            mutate<CreateRevision>("create_revision", {
                parent_change_ids: [rev.change_id],
            });
        } else {
            mutate<CheckoutRevision>("checkout_revision", {
                change_id: rev.change_id,
            });
        }
    }
</script>

<button
    class="unbutton layout"
    class:selected
    class:conflict={rev.has_conflict}
    class:context={rev == $currentContext}
    on:click={onSelect}
    on:contextmenu={onMenu}
    on:dblclick={onEdit}>
    <IdSpan type="change" id={rev.change_id} context={rev == $currentContext} />

    <span
        class="desc truncate"
        class:indescribable={rev.description.lines[0] == ""}>
        {rev.description.lines[0] == ""
            ? "(no description set)"
            : rev.description.lines[0]}
    </span>

    <span class="email truncate">{rev.author.email}</span>

    <span class="tags">
        {#each rev.branches.filter((b) => b.remote == null || !b.is_synced) as ref}
            <code class="tag" class:conflict={ref.has_conflict}>
                {ref.remote == null ? ref.name : `${ref.name}@${ref.remote}`}
            </code>
        {/each}
    </span>
</button>

<style>
    .layout {
        /* layout summary components along a text line */
        height: 100%;
        width: 100%;
        display: grid;
        grid-template-areas: ". desc tags";
        grid-template-columns: auto 1fr auto;
        align-items: baseline;
        gap: 6px;

        /* skip past svg lines when used in a graph */
        padding-left: var(--leftpad);
    }

    .layout > :global(span) {
        line-height: 27px;
    }

    .layout.selected {
        background: var(--ctp-base);
    }

    .layout.context {
        color: var(--ctp-rosewater);
    }

    .desc {
        grid-area: desc;
    }

    .desc.indescribable {
        color: var(--ctp-subtext0);
    }

    .email {
        display: none;
        grid-area: email;
        color: var(--ctp-surface2);
        text-align: right;
    }

    .tags {
        grid-area: tags;
        display: flex;
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

    /* multiple elements can have this */
    .truncate {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
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

    @media (width >= 1920px) {
        .layout {
            grid-template-areas: ". desc tags email";
            grid-template-columns: auto auto 1fr auto;
            gap: 9px;
        }

        .email {
            display: initial;
        }
    }
</style>
