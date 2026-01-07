<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { ChangeHunk } from "../messages/ChangeHunk";
    import type { TreePath } from "../messages/TreePath";
    import type { Operand } from "../messages/Operand";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    export let header: RevHeader | null;
    export let path: TreePath;
    export let hunk: ChangeHunk;

    let operand: Operand | null =
        header == null
            ? null
            : {
                  type: "Change",
                  header,
                  path,
                  hunk,
              };

    function getHunkDescription(hunk: ChangeHunk): string {
        return `@@ -${hunk.location.from_file.start},${hunk.location.from_file.len} +${hunk.location.to_file.start},${hunk.location.to_file.len} @@`;
    }
</script>

<Object {operand} conflicted={false} label={getHunkDescription(hunk)} let:context let:hint={dragHint}>
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
    }

    .target {
        color: black;
        background: var(--ctp-flamingo);
    }
</style>
