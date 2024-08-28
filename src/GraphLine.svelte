<script lang="ts">
    import type { EnhancedLine } from "./GraphLog.svelte";
    import type { Operand } from "./messages/Operand";
    import Zone from "./objects/Zone.svelte";
    import { currentTarget } from "./stores";

    export let line: EnhancedLine;

    let isMerge = line.type == "ToIntersection";
    let allowEarlyBreak = line.type == "FromNode";
    let c1 = line.source[0];
    let c2 = line.target[0];
    let r1 = line.source[1];
    let r2 = line.target[1];

    let operand: Operand = { type: "Parent", header: line.parent, child: line.child };

    // draw path downward, from child to parent
    let path: string;

    let childY = r1 * 30 + 21;
    let parentY = r2 * 30 + 9;

    let blockX: number;
    let blockY: number;
    let blockW: number;
    let blockH: number;

    if (isMerge) {
        // instead of a parent, we have a mergepoint
        let childX = c1 * 18 + 9;
        let mergeX = c2 * 18 + 9;
        let midY = c2 > c1 ? childY + 9 : parentY - 9;
        let radius = c2 > c1 ? 6 : -6;
        let sweep = c2 > c1 ? 0 : 1;
        path = `M${childX},${childY} 
            L${childX},${midY - 6} 
            A6,6,0,0,${sweep},${childX + radius},${midY}
            L${mergeX - radius},${midY}
            A6,6,0,0,${1 - sweep},${mergeX},${midY + 6} 
            L${mergeX},${parentY}`;

        blockX = c1 < c2 ? c1 * 18 + 2 : c2 * 18 + 2;
        blockY = r1 * 30 + 22;
        blockW = c1 < c2 ? (c2 - c1 + 1) * 18 - 5 : (c1 - c2 + 1) * 18 - 5;
        blockH = 14;
    } else if (c1 == c2) {
        // same-column, straight line
        let x = c1 * 18 + 9;

        path = `M${x},${childY} L${x},${parentY}`;

        blockX = c1 * 18 + 2;
        blockY = r1 * 30 + 21;
        blockW = 14;
        blockH = (r2 - r1) * 30 - 12;
    } else {
        // different-column, curved line
        let childX = c1 * 18 + 9;
        let parentX = c2 * 18 + 9;
        let midY = allowEarlyBreak && c1 > c2 ? parentY - 9 : childY + 9;
        let radius = c2 > c1 ? 6 : -6;
        let sweep = c2 > c1 ? 0 : 1;
        path = `M${childX},${childY} 
            L${childX},${midY - 6} 
            A6,6,0,0,${sweep},${childX + radius},${midY}
            L${parentX - radius},${midY}
            A6,6,0,0,${1 - sweep},${parentX},${midY + 6} 
            L${parentX},${parentY}`;

        blockX = c1 < c2 ? c1 * 18 + 16 : c2 * 18 + 16;
        blockY = r1 * 30 + 22;
        blockW = c1 < c2 ? (c2 - c1) * 18 - 14 : (c1 - c2) * 18 - 14;
        blockH = 14;
    }
</script>

{#if !line.indirect}
    <foreignObject x={blockX} y={blockY} width={blockW} height={blockH}>
        <Zone {operand} let:target>
            <div class="backdrop" class:target />
        </Zone>
    </foreignObject>
{/if}

<path d={path} fill="none" stroke-dasharray={line.indirect ? "1,2" : "none"} class:target={$currentTarget == operand} />

<style>
    path {
        pointer-events: none;
        stroke: var(--ctp-blue);
    }

    foreignObject > :global(*) {
        height: 100%;
    }

    .backdrop {
        width: 100%;
        height: 100%;
    }

    .target {
        stroke: black;
        background-color: var(--ctp-flamingo);
    }
</style>
