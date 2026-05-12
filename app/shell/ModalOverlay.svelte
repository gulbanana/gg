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

        background: var(--gg-colors-overlayBackground);
        backdrop-filter: blur(var(--gg-colors-overlayBackdropBlur));
        pointer-events: auto;

        display: grid;
        grid-template-columns: minmax(16.6%, 1fr) auto minmax(16.6%, 1fr);
        grid-template-rows: 1fr auto 1fr;
    }
</style>
