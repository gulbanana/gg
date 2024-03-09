<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import { currentContext } from "./stores.js";

    export let rev: RevHeader;

    let context = false;
    $: context =
        $currentContext?.type == "Revision" && rev == $currentContext.rev;
</script>

{#if rev.is_immutable}
    <circle class:context cx="9" cy="15" r="6" />
{:else}
    <circle class:context cx="9" cy="15" r="6" class="mutable" />
    {#if rev.is_working_copy}
        <circle class:context cx="9" cy="15" r="3" />
    {/if}
{/if}

<style>
    circle {
        pointer-events: none;
        stroke: var(--ctp-blue);
        fill: var(--ctp-blue);
    }

    .context {
        stroke: var(--ctp-rosewater);
        fill: var(--ctp-rosewater);
    }

    .mutable {
        fill: none;
    }
</style>
