<script lang="ts">
    import type { LogLine } from "./messages/LogLine";

    export let line: LogLine;

    let isMerge = line.type == "ToIntersection";
    let allowEarlyBreak = line.type == "FromNode";
    let c1 = line.source[0];
    let c2 = line.target[0];
    let r1 = line.source[1];
    let r2 = line.target[1];

    // draw path downward, from child to parent
    let path: string;

    let childY = r1 * 30 + 21;
    let parentY = r2 * 30 + 9;

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
    } else if (c1 == c2) {
        // same-column, straight line
        let x = c1 * 18 + 9;
        path = `M${x},${childY} L${x},${parentY}`;
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
    }
</script>

<path d={path} fill="none" stroke-dasharray={line.indirect ? "1,2" : "none"} />

<style>
    path {
        pointer-events: none;
    }
</style>
