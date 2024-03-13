<!--
@component
A drop target for direct-manipulation objects.
-->

<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import BinaryMutator from "../mutators/BinaryMutator";
    import { currentSource, currentTarget } from "../stores";

    interface $$Slots {
        default: { target: boolean };
    }

    export let operand: Operand;

    function onDragEnter(event: DragEvent) {
        event.stopPropagation();

        let mutator = new BinaryMutator($currentSource, operand);
        if (mutator.canDrop().type == "yes") {
            event.preventDefault();
            $currentTarget = operand;
        }
    }

    function onDragOver(event: DragEvent) {
        event.stopPropagation();

        let mutator = new BinaryMutator($currentSource, operand);
        if (mutator.canDrop().type == "yes") {
            event.preventDefault();
            if ($currentTarget != operand) {
                $currentTarget = operand;
            }
        }
    }

    function onDragLeave(event: DragEvent) {
        $currentTarget = null;
    }

    function onDrop(event: DragEvent) {
        event.stopPropagation();
        $currentTarget = null;

        let mutator = new BinaryMutator($currentSource, operand);
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
    <slot target={$currentTarget == operand} />
</div>

<style>
    div {
        width: 100%;
        pointer-events: all;
    }
</style>
