<script lang="ts">
    export let c1: number;
    export let r1: number;
    export let c2: number;
    export let r2: number;
    export let has_elisions: boolean;

    // draw path downward, from child to parent
    let path: string;

    let childY = r1 * 30 + 21;
    let parentY = r2 * 30 + 9;

    // same-column, straight line
    if (c1 == c2) {
        let x = c1 * 18 + 9;
        path = `M${x},${childY} L${x},${parentY}`;
    // different-column, curved line
    } else {
        let childX = c1 * 18 + 9;
        let parentX = c2 * 18 + 9;
        let midY = c2 > c1 ? childY + 9 : parentY - 9;
        let radius = c2 > c1 ? 6 : -6;
        let sweep = c2 > c1 ? 0 : 1;
        path = `M${childX},${childY} 
            L${childX},${midY-6} 
            A6,6,0,0,${sweep},${childX+radius},${midY}
            L${parentX-radius},${midY}
            A6,6,0,0,${1-sweep},${parentX},${midY+6} 
            L${parentX},${parentY}`;
    }
</script>

<path d={path} fill="none" stroke-dasharray={has_elisions ? "1,2" : "none"} />
