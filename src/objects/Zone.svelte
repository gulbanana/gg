<!--
@component
A drop target for direct-manipulation objects.
-->

<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import BinaryMutator from "../mutators/BinaryMutator";
    import { currentDrag } from "../stores";

    interface $$Slots {
        default: { target: boolean };
    }

    export let operand: Operand;

    let target = false;

    function onDragEnter(event: DragEvent) {
        event.stopPropagation();

        let mutator = new BinaryMutator($currentDrag, operand);
        if (mutator.canDrop().type == "yes") {
            event.preventDefault();
            target = true;
        }
    }

    function onDragOver(event: DragEvent) {
        event.stopPropagation();

        let mutator = new BinaryMutator($currentDrag, operand);
        if (mutator.canDrop().type == "yes") {
            event.preventDefault();
        }
    }

    function onDragLeave(event: DragEvent) {
        target = false;
    }

    function onDrop(event: DragEvent) {
        target = false;

        let mutator = new BinaryMutator($currentDrag, operand);
        if (mutator.canDrop().type == "yes") {
            mutator.doDrop();
        }
    }
</script>

<div
    role="presentation"
    on:dragenter={onDragEnter}
    on:dragover={onDragOver}
    on:dragleave={onDragLeave}
    on:drop={onDrop}>
    <slot {target} />
</div>

<style>
    div {
        width: 100%;
        pointer-events: all;
    }
</style>
