<script lang="ts">
    import type { ProgressEvent } from "../messages/ProgressEvent";

    export let progress: ProgressEvent | undefined;

    let label = "Working...";
    let value: number | undefined = undefined;

    $: if (progress) {
        if (progress.type === "Progress") {
            value = progress.overall_percent;
        } else if (progress.type === "Message") {
            label = progress.text;
        }
    }
</script>

<div class="toolbar-progress">
    <span class="label">{label}</span>
    <div class="bar-track">
        {#if value !== undefined}
            <div class="bar-fill" style="width: {value}%"></div>
        {:else}
            <div class="bar-fill indeterminate"></div>
        {/if}
    </div>
</div>

<style>
    .toolbar-progress {
        margin-left: auto;
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        justify-content: center;
        gap: 2px;
        padding: 0 4px;
        min-width: 120px;
        max-width: 300px;
    }

    .label {
        font-size: var(--gg-text-sizeMd);
        color: var(--gg-colors-foregroundMuted);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 100%;
    }

    .bar-track {
        width: 100%;
        height: 3px;
        background: var(--gg-colors-surfaceStrong);
        border-radius: 1.5px;
        overflow: hidden;
    }

    .bar-fill {
        height: 100%;
        background: var(--gg-colors-primary);
        border-radius: 1.5px;
        transition: width 0.2s ease;
    }

    .bar-fill.indeterminate {
        width: 40%;
        animation: indeterminate 1.5s infinite ease-in-out;
    }

    @keyframes indeterminate {
        0% {
            transform: translateX(-100%);
        }
        100% {
            transform: translateX(350%);
        }
    }
</style>
