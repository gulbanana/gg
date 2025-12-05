<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { ChangeHunk } from "../messages/ChangeHunk";
    import type { TreePath } from "../messages/TreePath";
    import type { Operand } from "../messages/Operand";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    let { header, path, hunk }: {
        header: RevHeader;
        path: TreePath;
        hunk: ChangeHunk;
    } = $props();

    let operand = $derived<Operand>({
        type: "Change",
        header,
        path,
        hunk,
    });

    function getHunkDescription(hunk: ChangeHunk): string {
        return `@@ -${hunk.location.from_file.start},${hunk.location.from_file.len} +${hunk.location.to_file.start},${hunk.location.to_file.len} @@`;
    }
</script>

<Object {operand} conflicted={false} label={getHunkDescription(hunk)}>
    {#snippet children({ hint: dragHint })}
        <Zone {operand}>
            {#snippet children({ target, hint: dropHint })}
                <div class="hunk" class:target>
                    {dragHint ?? dropHint ?? getHunkDescription(hunk)}
                </div>
            {/snippet}
        </Zone>
    {/snippet}
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
