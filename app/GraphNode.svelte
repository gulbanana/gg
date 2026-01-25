<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import { currentContext } from "./stores.js";

    export let header: RevHeader;

    let context = false;
    $: context = $currentContext?.type == "Revision" && header == $currentContext.header;
</script>

{#if header.is_immutable}
    {#if header.is_working_copy}
        <circle class="wc" class:context cx="9" cy="15" r="6" />
    {:else}
        <circle class:context cx="9" cy="15" r="6" />
    {/if}
{:else}
    <circle class:context cx="9" cy="15" r="6" class="mutable" />
    {#if header.is_working_copy}
        <circle class="wc" class:context cx="9" cy="15" r="3" />
    {/if}
{/if}

<style>
    circle {
        pointer-events: none;
        stroke: var(--ctp-blue);
        fill: var(--ctp-blue);
    }

    .wc {
        stroke: var(--ctp-green);
        fill: var(--ctp-green);
    }

    .context {
        stroke: var(--ctp-rosewater);
        fill: var(--ctp-rosewater);
    }

    .mutable {
        fill: none;
    }
</style>
