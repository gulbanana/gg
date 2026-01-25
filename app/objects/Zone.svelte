<!--
@component
A drop target for direct-manipulation objects.
-->

<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import BinaryMutator from "../mutators/BinaryMutator";
    import { currentSource, currentTarget, ignoreToggled } from "../stores";

    interface $$Slots {
        default: { target: boolean; hint: string | null };
    }

    export let operand: Operand | null;
    export let alwaysTarget: boolean = false;

    let dropHint: string | null = null;
    let target = false;
    $: target = match($currentTarget);

    function match(target: Operand | null): boolean {
        return (
            (operand && target == operand) ||
            (operand?.type == "Merge" && target?.type == "Merge" && operand.header.id.commit == target.header.id.commit)
        );
    }

    function onDragOver(event: DragEvent) {
        event.stopPropagation();

        let canDrop =
            operand == null
                ? { type: "no", hint: "" }
                : new BinaryMutator($currentSource!, operand, $ignoreToggled).canDrop();

        if (canDrop.type == "yes") {
            event.preventDefault();
            if (!match($currentTarget)) {
                $currentTarget = operand;
            }
            dropHint = null;
        } else if (canDrop.type == "maybe") {
            event.preventDefault();
            dropHint = canDrop.hint;
            if (alwaysTarget && !match($currentTarget)) {
                $currentTarget = operand;
            }
        }
    }

    function onDragLeave(event: DragEvent) {
        $currentTarget = null;
        dropHint = null;
    }

    function onDrop(event: DragEvent) {
        event.stopPropagation();

        if (operand) {
            let mutator = new BinaryMutator($currentSource!, operand, $ignoreToggled);
            if (mutator.canDrop().type == "yes") {
                mutator.doDrop();
            }
        }

        $currentSource = null;
        $currentTarget = null;
        dropHint = null;
    }
</script>

<div
    role="presentation"
    class="zone"
    class:hint={dropHint}
    on:dragenter={onDragOver}
    on:dragover={onDragOver}
    on:dragleave={onDragLeave}
    on:drop={onDrop}>
    <slot {target} hint={dropHint} />
</div>

<style>
    .zone {
        width: 100%;
        pointer-events: auto;
    }

    .hint {
        color: var(--ctp-peach);
    }
</style>
