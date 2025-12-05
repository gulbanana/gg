<script lang="ts">
    import type { Snippet } from "svelte";
    
    let { context, target, disconnected = false, immobile = false, tip = "", children }: { 
        context: boolean; 
        target: boolean; 
        disconnected?: boolean; 
        immobile?: boolean; 
        tip?: string;
        children?: Snippet;
    } = $props();
</script>

<div class="chip" class:context class:target class:disconnected class:immobile title={tip}>
    {@render children?.()}
</div>

<style>
    .chip {
        pointer-events: auto;

        font-family: var(--stack-code);
        font-size: smaller;

        height: 24px;
        line-height: 16px;

        display: flex;
        align-items: center;
        border: 1px solid var(--ctp-overlay1);
        border-radius: 12px;
        padding: 0 6px;
        background: var(--ctp-mantle);
        white-space: nowrap;
        gap: 3px;
    }

    .context {
        color: inherit;
        border-color: currentColor;
    }

    .target {
        background: var(--ctp-flamingo);
        color: black;
    }

    .disconnected {
        border-style: dashed;
    }

    .immobile {
        border-style: none;
    }
</style>
