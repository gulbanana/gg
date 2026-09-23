<script lang="ts">
</script>

<!-- keeps content clear of the macos traffic lights, leaving the gap as a window drag handle -->
<div class="inset">
    <div class="titlebar" data-tauri-drag-region></div>
    <slot />
</div>

<style>
    .inset {
        display: grid;
        grid-template-rows: var(--titlebar-height, 0px) minmax(0, 1fr);
        grid-template-columns: minmax(0, 1fr);
        overflow: hidden;
    }

    /* opt out of drop-transparency, or tauri never sees the mousedown */
    .titlebar {
        pointer-events: auto;
    }
</style>
