<script lang="ts">
    export let context: boolean;
    export let target: boolean;
    export let disconnected: boolean = false;
</script>

<div class="chip" class:context class:target class:disconnected>
    <slot />
</div>

<style>
    .chip {
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
</style>
