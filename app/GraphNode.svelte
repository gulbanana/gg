<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import { currentContext } from "./stores.js";

    export let header: RevHeader;

    let context = false;
    $: context = $currentContext?.type == "Revision" && header == $currentContext.header;
    $: wcClass = header.working_copy_of != null ? "other-wc" : "wc";
</script>

{#if header.is_immutable}
    {#if header.is_working_copy}
        <circle class={wcClass} class:context cx="9" cy="15" r="6" />
    {:else}
        <circle class:context cx="9" cy="15" r="6" />
    {/if}
{:else}
    <circle class:context cx="9" cy="15" r="6" class="mutable" />
    {#if header.is_working_copy}
        <circle class={wcClass} class:context cx="9" cy="15" r="3" />
    {/if}
{/if}

<style>
    circle {
        pointer-events: none;
        stroke: var(--gg-colors-immutableStroke);
        fill: var(--gg-colors-immutableFill);
    }

    .wc {
        stroke: var(--gg-colors-workingCopyStroke);
        fill: var(--gg-colors-workingCopyFill);
    }

    .other-wc {
        stroke: var(--gg-colors-warning);
        fill: var(--gg-colors-warning);
    }

    .context {
        stroke: var(--gg-colors-accent);
        fill: var(--gg-colors-accent);
    }

    .mutable {
        fill: var(--gg-colors-mutableFill);
    }
</style>
