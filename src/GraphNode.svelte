<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";

    export let revision: RevHeader;
</script>

{#if revision.is_immutable}
    <circle cx="9" cy="15" r="6" />
{:else}
    <circle cx="9" cy="15" r="6" class="mutable" />
    {#if revision.is_working_copy}
        <circle cx="9" cy="15" r="3" />
    {/if}
{/if}

<style>
    circle {
        pointer-events: none;
        stroke: var(--ctp-blue);
        fill: var(--ctp-blue);
    }

    .mutable {
        fill: none;
    }
</style>
