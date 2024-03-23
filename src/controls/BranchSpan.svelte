<script lang="ts">
    import type { StoreRef } from "../messages/StoreRef";

    export let ref: Extract<StoreRef, { type: "LocalBranch" } | { type: "RemoteBranch" }>;
</script>

<span class="ref">
    {ref.branch_name}{#if ref.type == "RemoteBranch"}@{ref.remote_name}{/if}
</span>

<style>
    .ref {
        pointer-events: auto;
        user-select: text;
        color: var(--ctp-subtext1);
        font-family: var(--stack-code);
    }
</style>
