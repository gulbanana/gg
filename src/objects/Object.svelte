<!--
@component
Core component for direct-manipulation objects. A drag&drop source.
-->

<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import { command } from "../ipc";
    import { currentContext, currentDrag } from "../stores";
    import { createEventDispatcher } from "svelte";

    interface $$Slots {
        default: { context: boolean };
    }

    interface $$Events {
        click: CustomEvent<MouseEvent>;
        dblclick: CustomEvent<MouseEvent>;
    }

    export let id: string = "";
    export let label: string;
    export let selected: boolean = false;
    export let conflicted: boolean;
    export let operand: Operand;

    let dragging = false;

    let dispatch = createEventDispatcher();

    function onClick(event: MouseEvent) {
        dispatch("click", event);
    }

    function onDoubleClick(event: MouseEvent) {
        dispatch("dblclick", event);
    }

    function onMenu(event: Event) {
        event.preventDefault();
        event.stopPropagation();

        currentContext.set(operand);
        command("forward_context_menu", { context: operand });
    }

    function onDragStart(event: DragEvent) {
        event.stopPropagation();
        dragging = true;
        $currentDrag = operand; // it would've been nice to just put this in the drag data but chrome says That's Insecure
        event.dataTransfer?.setData("text/plain", ""); // if we need more than one drag to be active, this could store a key
    }

    function onDragEnd() {
        dragging = false;
    }
</script>

<button
    {id}
    class:selected
    class:conflict={conflicted}
    class:context={dragging || $currentContext == operand}
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
    <slot context={dragging || $currentContext == operand} />
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
</style>
