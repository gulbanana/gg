<script lang="ts">
    import { hasModal } from "../stores";

    // the header shares space with the macos titlebar
    export let titlebar = false;
</script>

<section inert={$hasModal} class:titlebar>
    <div class="header">
        {#if titlebar}
            <div class="drag-region" data-tauri-drag-region></div>
        {/if}
        <slot name="header" />
    </div>
    <div class="body">
        <slot name="body" />
    </div>
</section>

<style>
    section {
        display: grid;
        grid-template-rows: 40px 1fr;
        grid-template-columns: 100%;
        overflow: hidden;
    }

    .header {
        border-bottom: 1px solid var(--ctp-overlay0);
        padding: 6px 6px 3px 6px;
    }

    .body {
        padding: 3px 6px 6px 6px;
        display: grid;
        overflow: hidden;
    }

    .drag-region {
        display: none;
    }

    /* the body lines up with content below the titlebar, which gets 6px of header padding to our 3px */
    :global(.overlay-titlebar) .titlebar {
        grid-template-rows: calc(var(--titlebar-height) + 3px) 1fr;
    }

    /* the content keeps its usual offset, so it overhangs the shortened row by a pixel */
    :global(.overlay-titlebar) .titlebar > .header {
        position: relative;
        z-index: 0;
        border-bottom: none;
    }

    /* header content is click-through, so the window can be dragged from between its controls */
    :global(.overlay-titlebar) .titlebar > .header > :global(:not(.drag-region)) {
        pointer-events: none;
    }

    :global(.overlay-titlebar) .titlebar .drag-region {
        display: block;
        position: absolute;
        inset: 0;
        z-index: -1;
        pointer-events: auto;
    }
</style>
