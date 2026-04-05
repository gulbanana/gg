<!--
@component
Core component for direct-manipulation objects. A drag&drop source.
-->

<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import { trigger, isTauri } from "../ipc";
    import { currentContext, currentSource, selectionHeaders, hasMenu, ignoreToggled } from "../stores";
    import { createEventDispatcher } from "svelte";
    import { get } from "svelte/store";
    import BinaryMutator from "../mutators/BinaryMutator";

    interface $$Slots {
        default: { context: boolean; hint: string | null };
    }

    interface $$Events {
        click: CustomEvent<MouseEvent>;
        dblclick: CustomEvent<MouseEvent>;
    }

    export let suffix: string | null = null;
    export let label: string;
    export let selected: boolean = false;
    export let conflicted: boolean;
    export let operand: Operand | null;

    let dispatch = createEventDispatcher();

    let id = suffix == null ? null : operand == null ? null : `${operand.type}-${suffix}`;
    let dragging: boolean;
    let dragHint: string | null = null;

    function onClick(event: MouseEvent) {
        dispatch("click", event);
    }

    function onDoubleClick(event: MouseEvent) {
        dispatch("dblclick", event);
    }

    function getEffectiveOperand(): Operand | null {
        if (operand?.type == "Revision") {
            let headers = get(selectionHeaders);
            if (
                headers &&
                headers.length > 1 &&
                headers.some((h) => h.id.commit.hex === operand.header.id.commit.hex)
            ) {
                return { type: "Revisions", headers };
            }
        }

        return operand;
    }

    function onMenu(event: Event) {
        if (operand?.type == "Ref" || operand?.type == "Change" || operand?.type == "Revision" || operand?.type == "Workspace") {
            event.preventDefault();
            event.stopPropagation();

            let effectiveOperand = getEffectiveOperand();
            currentContext.set(effectiveOperand);

            if (isTauri()) {
                trigger("forward_context_menu", { context: effectiveOperand });
            } else {
                const mouseEvent = event as MouseEvent;
                hasMenu.set({ x: mouseEvent.clientX, y: mouseEvent.clientY });
            }
        }
    }

    function onDragStart(event: DragEvent) {
        currentContext.set(null);
        event.stopPropagation();

        let effectiveOperand = getEffectiveOperand();
        let canDrag =
            effectiveOperand == null
                ? { type: "no", hint: "" }
                : new BinaryMutator(effectiveOperand, null, $ignoreToggled).canDrag();

        if (canDrag.type == "no") {
            return;
        } else {
            event.dataTransfer?.setData("text/plain", ""); // if we need more than one drag to be active, this could store a key
            $currentSource = effectiveOperand; // it would've been nice to just put this in the drag data but chrome says That's Insecure
            dragging = true;

            if (canDrag.type == "maybe") {
                dragHint = canDrag.hint;
                let empty = document.createElement("div");
                event.dataTransfer?.setDragImage(empty, 0, 0);
            }
        }
    }

    function onDragEnd() {
        $currentSource = null;
        dragging = false;
        dragHint = null;
    }

    // check if this operand is part of the current context
    function isInContext(ctx: Operand | null, op: Operand | null): boolean {
        if (!ctx || !op) return false;
        if (ctx === op) return true;
        if (ctx.type === "Revisions" && op.type === "Revision") {
            return ctx.headers.some((h) => h.id.commit.hex === op.header.id.commit.hex);
        }
        return false;
    }

    // check if this operand is part of the current drag source
    function isInSource(src: Operand | null, op: Operand | null): boolean {
        if (!src || !op) return false;
        if (src === op) return true;
        if (src.type === "Revisions" && op.type === "Revision") {
            return src.headers.some((h) => h.id.commit.hex === op.header.id.commit.hex);
        }
        return false;
    }

    $: inContext = isInContext($currentContext, operand);
    $: inSource = isInSource($currentSource, operand);
</script>

<button
    {id}
    class:selected
    class:conflict={conflicted}
    class:context={dragging || inContext || inSource}
    class:hint={dragHint}
    tabindex="-1"
    draggable={operand != null}
    role="option"
    aria-label={label}
    aria-selected={selected}
    on:click={onClick}
    on:dblclick={onDoubleClick}
    on:contextmenu={onMenu}
    on:dragstart={onDragStart}
    on:dragend={onDragEnd}>
    <slot context={dragging || inContext || inSource} hint={dragHint} />
</button>

<style>
    button {
        /* reset button styles */
        background: transparent;
        border: none;
        margin: 0;
        padding: 0;
        color: inherit;
        text-align: left;

        width: 100%;
        display: flex;
        align-items: center;
    }

    button[draggable="true"] {
        cursor: grab;
    }

    .selected {
        background: var(--gg-colors-selectionBackground);
        color: var(--gg-colors-selectionForeground);
    }

    .selected :global(.id) {
        color: var(--gg-colors-selectionForeground);
    }

    .selected :global(.prefix) {
        color: var(--gg-colors-highlight);
    }

    .selected :global(.chip) {
        background: var(--gg-colors-selectionBackground);
        color: var(--gg-colors-selectionForeground);
    }

    .conflict {
        background: repeating-linear-gradient(
            120deg,
            transparent 0px,
            transparent 12px,
            color-mix(in srgb, var(--gg-colors-conflictStroke) 40%, var(--gg-colors-background)) 12px,
            color-mix(in srgb, var(--gg-colors-conflictStroke) 40%, var(--gg-colors-background)) 15px
        );
    }

    .conflict :global(.feather) {
        color: var(--gg-colors-conflict);
    }

    .selected.conflict {
        background: repeating-linear-gradient(
            120deg,
            var(--gg-colors-conflictStroke) 0px,
            var(--gg-colors-conflictStroke) 12px,
            color-mix(in srgb, var(--gg-colors-conflictStroke) 80%, var(--gg-colors-background)) 12px,
            color-mix(in srgb, var(--gg-colors-conflictStroke) 80%, var(--gg-colors-background)) 15px
        );
        color: var(--gg-colors-warningContent);
    }

    .selected.conflict :global(.id) {
        color: var(--gg-colors-foreground);
    }

    .selected.conflict :global(.prefix) {
        color: var(--gg-colors-accent);
    }

    .selected.conflict :global(.feather) {
        color: var(--gg-colors-conflictAlt);
    }

    .context {
        color: var(--gg-colors-background);
    }

    .hint {
        color: var(--gg-colors-highlight);
    }

    .selected :global(.feather.remove) {
        stroke: var(--gg-colors-removedAlt);
    }

    .selected :global(.feather.add) {
        stroke: var(--gg-colors-addedAlt);
    }

    .selected :global(.feather.change) {
        stroke: var(--gg-colors-modifiedAlt);
    }

    .selected :global(.desc.indescribable) {
        color: var(--gg-colors-selectionForegroundMuted);
    }

    .selected :global(.author) {
        color: var(--gg-colors-selectionForegroundMuted);
    }
</style>
