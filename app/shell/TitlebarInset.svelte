<script lang="ts">
    import { onMount } from "svelte";
    import { trigger } from "../ipc";

    let titlebar: HTMLElement;

    // the native title is drawn in this gap, so the backend shortens it to fit
    onMount(() => {
        if (!document.documentElement.classList.contains("overlay-titlebar")) return;

        let lastLimit = -1;
        let observer = new ResizeObserver(() => {
            let limit = Math.round(titlebar.getBoundingClientRect().right);
            if (limit != lastLimit) {
                lastLimit = limit;
                trigger("set_title_limit", { limit });
            }
        });
        observer.observe(titlebar);
        return () => observer.disconnect();
    });
</script>

<!-- keeps content clear of the macos traffic lights, leaving the gap as a window drag handle -->
<div class="inset">
    <div class="titlebar" data-tauri-drag-region bind:this={titlebar}></div>
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
