<script lang="ts">
    import type { StoreRef } from "../messages/StoreRef";

    export let ref: Extract<StoreRef, { type: "LocalBookmark" } | { type: "RemoteBookmark" }>;
</script>

<span class="ref">
    {ref.branch_name}{#if ref.type == "RemoteBookmark"}@{ref.remote_name}{/if}
</span>

<style>
    .ref {
        pointer-events: auto;
        user-select: text;
        color: var(--ctp-subtext1);
        font-family: var(--stack-code);
    }
</style>
