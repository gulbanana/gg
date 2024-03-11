<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import type { CheckoutRevision } from "./messages/CheckoutRevision";
    import type { CreateRevision } from "./messages/CreateRevision";
    import { revisionSelectEvent, currentContext } from "./stores.js";
    import { command, mutate } from "./ipc";
    import IdSpan from "./IdSpan.svelte";
    import type { MenuContext } from "./messages/MenuContext";
    import RefSummary from "./BranchSummary.svelte";

    export let rev: RevHeader;
    export let selected: boolean; // same as the imported event, but parent may want to force a value
    export let prefix: string;

    let is_context = false;
    $: is_context =
        $currentContext?.type == "Revision" && rev == $currentContext.rev;

    function onSelect() {
        revisionSelectEvent.set(rev);
    }

    function onMenu(event: Event) {
        event.preventDefault();
        event.stopPropagation();

        let context: MenuContext = { type: "Revision", rev };
        currentContext.set(context);

        command("forward_context_menu", { context });
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
    id="{prefix}-{rev.change_id.prefix}"
    class="unbutton layout"
    class:selected
    class:conflict={rev.has_conflict}
    class:context={is_context}
    tabindex="-1"
    role="option"
    aria-selected={selected}
    aria-label={rev.description.lines[0]}
    on:click={onSelect}
    on:contextmenu={onMenu}
    on:dblclick={onEdit}>
    <IdSpan type="change" id={rev.change_id} context={is_context} />

    <span
        class="desc truncate"
        class:indescribable={rev.description.lines[0] == ""}>
        {rev.description.lines[0] == ""
            ? "(no description set)"
            : rev.description.lines[0]}
    </span>

    <span class="email truncate">{rev.author.email}</span>

    <span class="refs">
        {#each rev.branches.filter((b) => b.type == "LocalBranch" || !b.is_synced || !b.is_tracked) as ref}
            <RefSummary {rev} {ref} />
        {/each}
    </span>
</button>

<style>
    .layout {
        cursor: pointer;

        /* layout summary components along a text line */
        height: 100%;
        width: 100%;
        display: grid;
        grid-template-areas: ". desc refs";
        grid-template-columns: auto 1fr auto;
        align-items: baseline;
        gap: 6px;

        /* skip past svg lines when used in a graph */
        padding-left: var(--leftpad);
    }

    .layout > :global(span) {
        line-height: 27px;
    }

    .layout.context {
        color: var(--ctp-rosewater);
    }

    .layout.conflict {
        background: repeating-linear-gradient(
            120deg,
            transparent 0px,
            transparent 12px,
            var(--ctp-surface0) 12px,
            var(--ctp-surface0) 15px
        );
    }

    .layout.selected {
        background: var(--ctp-base);
    }

    .layout.selected.conflict {
        background: repeating-linear-gradient(
            120deg,
            var(--ctp-surface0) 0px,
            var(--ctp-surface0) 12px,
            var(--ctp-base) 12px,
            var(--ctp-base) 15px
        );
    }

    .desc {
        grid-area: desc;
    }

    .desc.indescribable {
        color: var(--ctp-subtext0);
    }

    .layout.context > .desc.indescribable {
        color: var(--ctp-rosewater);
    }

    .email {
        display: none;
        grid-area: email;
        color: var(--ctp-surface2);
        text-align: right;
    }

    .refs {
        grid-area: refs;
        align-self: center;
        display: flex;
    }

    /* multiple elements can have this */
    .truncate {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    @media (width >= 1680px) {
        .layout {
            grid-template-areas: ". desc refs email";
            grid-template-columns: auto auto 1fr auto;
            gap: 9px;
        }

        .email {
            display: initial;
        }
    }
</style>
