<script lang="ts">
    import { onMount } from "svelte";
    import { hasModal } from "../stores";

    onMount(() => {
        $hasModal = true;
        return () => {
            $hasModal = false;
        };
    });
</script>

<div id="overlay">
    <!-- the overlay covers the window's drag handles, so it needs its own -->
    <div class="titlebar" data-tauri-drag-region></div>
    <slot />
</div>

<style>
    #overlay {
        z-index: 1;
        position: absolute;
        top: 0;
        right: 0;
        bottom: 33px;
        left: 0;

        background: rgb(var(--ctp-overlay1-rgb) / 40%);
        pointer-events: auto;

        display: grid;
        grid-template-columns: minmax(16.6%, 1fr) auto minmax(16.6%, 1fr);
        grid-template-rows: 1fr auto 2fr;
    }

    .titlebar {
        position: absolute;
        top: 0;
        right: 0;
        left: 0;
        height: var(--titlebar-height, 0px);
        pointer-events: auto;
    }
</style>
