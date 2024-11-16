<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { Operand } from "../messages/Operand";
    import { currentTarget, revisionSelectEvent } from "../stores.js";
    import IdSpan from "../controls/IdSpan.svelte";
    import BranchObject from "./BranchObject.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";
    import RevisionMutator from "../mutators/RevisionMutator";
    import TagObject from "./TagObject.svelte";
    import AuthorSpan from "../controls/AuthorSpan.svelte";

    export let header: RevHeader;
    export let child: RevHeader | null = null;
    export let selected: boolean; // same as the imported event, but parent may want to force a value
    export let noBranches: boolean = false;

    let operand: Operand = child ? { type: "Parent", header, child } : { type: "Revision", header };

    function onSelect() {
        revisionSelectEvent.set(header);
    }

    function onEdit() {
        new RevisionMutator(header).onEdit();
    }
</script>

<Object
    {operand}
    suffix={header.id.commit.prefix}
    conflicted={header.has_conflict}
    {selected}
    label={header.description.lines[0]}
    on:click={onSelect}
    on:dblclick={onEdit}
    let:context
    let:hint={dragHint}>
    {#if child}
        <!-- Parents aren't a drop target -->
        <div class="layout">
            <IdSpan
                id={header.id.change}
                pronoun={context ||
                    ($currentTarget?.type == "Merge" &&
                        $currentTarget.header.parent_ids.findIndex((id) => id.hex == header.id.commit.hex) != -1)} />

            <span class="text desc truncate" class:indescribable={!context && header.description.lines[0] == ""}>
                {dragHint ?? (header.description.lines[0] == "" ? "(no description set)" : header.description.lines[0])}
            </span>

            <span class="email"><AuthorSpan author={header.author} /></span>

            <span class="refs">
                {#each header.refs as ref}
                    {#if ref.type != "Tag"}
                        {#if !noBranches && (ref.type == "LocalBookmark" || !ref.is_synced || !ref.is_tracked)}
                            <div>
                                <BranchObject {header} {ref} />
                            </div>
                        {/if}
                    {:else}
                        <div>
                            <TagObject {header} {ref} />
                        </div>
                    {/if}
                {/each}
            </span>
        </div>
    {:else}
        <Zone {operand} let:target let:hint={dropHint}>
            <div class="layout" class:target>
                <IdSpan id={header.id.change} pronoun={context || target || dropHint != null} />

                <span class="text desc truncate" class:indescribable={!context && header.description.lines[0] == ""}>
                    {dragHint ??
                        dropHint ??
                        (header.description.lines[0] == "" ? "(no description set)" : header.description.lines[0])}
                </span>

                <span class="email"><AuthorSpan author={header.author} /></span>

                <span class="refs">
                    {#each header.refs as ref}
                        {#if ref.type != "Tag"}
                            {#if ref.type == "LocalBookmark" || !ref.is_synced || !ref.is_tracked}
                                <div>
                                    <BranchObject {header} {ref} />
                                </div>
                            {/if}
                        {:else}
                            <div>
                                <TagObject {header} {ref} />
                            </div>
                        {/if}
                    {/each}
                </span>
            </div>
        </Zone>
    {/if}
</Object>

<style>
    .layout {
        /* layout summary components along a text line */
        width: 100%;
        height: 30px;
        display: grid;
        grid-template-areas: ". desc refs";
        grid-template-columns: auto 1fr auto;
        align-items: baseline;
        gap: 6px;

        /* skip past svg lines when used in a graph */
        padding-left: var(--leftpad);
    }

    .layout.target {
        background: var(--ctp-flamingo);
        color: black;
    }

    .layout > :global(span) {
        line-height: 27px;
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
        text-align: right;
    }

    .refs {
        grid-area: refs;
        align-self: center;
        display: flex;
        justify-content: end;
        gap: 3px;
        color: var(--ctp-text);
    }

    /* multiple elements can have these */
    .truncate {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .text {
        pointer-events: none;
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
