
<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { ChangeHunk } from "../messages/ChangeHunk";
    import type { Operand } from "../messages/Operand";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    export let header: RevHeader;
    export let path: string;
    export let hunk: ChangeHunk;

    let operand: Operand = {
        type: "Hunk",
        header,
        path,
        hunk,
        conflicted: false
    };

    function getHunkDescription(hunk: ChangeHunk): string {
        return `@@ -${hunk.location.from_file.start},${hunk.location.from_file.len} +${hunk.location.to_file.start},${hunk.location.to_file.len} @@`;
    }
</script>

<Object {operand} conflicted={operand.conflicted} label={getHunkDescription(hunk)} let:context let:hint={dragHint}>
    <Zone {operand} let:target let:hint={dropHint}>
        <div class="hunk" class:target>
            {dragHint ?? dropHint ?? getHunkDescription(hunk)}
        </div>
    </Zone>
</Object>

<style>
    .hunk {
        margin: 0;
        text-align: center;
        background: var(--ctp-mantle);
        padding: 4px;
        cursor: grab;
    }

    .target {
        color: black;
        background: var(--ctp-flamingo);
    }
</style>
