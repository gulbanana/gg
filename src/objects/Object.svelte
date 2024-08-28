<!--
@component
Core component for direct-manipulation objects. A drag&drop source.
-->

<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import { trigger } from "../ipc";
    import { currentContext, currentSource } from "../stores";
    import { createEventDispatcher } from "svelte";
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
    export let operand: Operand;

    let dispatch = createEventDispatcher();

    let id = suffix == null ? null : `${operand.type}-${suffix}`;
    let dragging: boolean;
    let dragHint: string | null = null;

    function onClick(event: MouseEvent) {
        dispatch("click", event);
    }

    function onDoubleClick(event: MouseEvent) {
        dispatch("dblclick", event);
    }

    function onMenu(event: Event) {
        if (operand.type == "Ref" || operand.type == "Change" || operand.type == "Revision") {
            event.preventDefault();
            event.stopPropagation();

            currentContext.set(operand);
            trigger("forward_context_menu", { context: operand });
        }
    }

    function onDragStart(event: DragEvent) {
        currentContext.set(null);
        event.stopPropagation();

        let canDrag = BinaryMutator.canDrag(operand);

        if (canDrag.type == "no") {
            return;
        } else {
            event.dataTransfer?.setData("text/plain", ""); // if we need more than one drag to be active, this could store a key
            $currentSource = operand; // it would've been nice to just put this in the drag data but chrome says That's Insecure
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
</script>

<button
    {id}
    class:selected
    class:conflict={conflicted}
    class:context={dragging || $currentContext == operand}
    class:hint={dragHint}
    tabindex="-1"
    draggable="true"
    role="option"
    aria-label={label}
    aria-selected={selected}
    on:click={onClick}
    on:dblclick={onDoubleClick}
    on:contextmenu={onMenu}
    on:dragstart={onDragStart}
    on:dragend={onDragEnd}>
    <slot context={dragging || $currentContext == operand} hint={dragHint} />
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

        cursor: pointer;
        width: 100%;
        display: flex;
        align-items: center;
    }

    .selected {
        background: var(--ctp-base);
    }

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

    .context {
        color: var(--ctp-rosewater);
    }

    .hint {
        color: var(--ctp-peach);
    }
</style>
