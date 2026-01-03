<script lang="ts">
    export let label: string = "Working...";
    export let value: number | undefined = undefined;
</script>

<div class="progress-bar">
    {#if value !== undefined}
        <progress {value} max={100}></progress>
    {:else}
        <progress></progress>
    {/if}
    <p class="progress-label">{label}</p>
</div>

<style>
    .progress-bar {
        display: grid;
        grid-template-areas: "overlap";
        align-items: center;
        justify-items: center;
    }

    .progress-bar > * {
        grid-area: 1/2/1/2;
    }

    .progress-label {
        color: buttontext;
        font-size: 16px;
        margin: 0;
    }

    progress {
        min-width: 600px;
        height: 24px;
        appearance: none;
        background: var(--ctp-flamingo);
        border-width: 1px;
        border-radius: 3px;
        border-color: var(--ctp-overlay0);
        box-shadow: 2px 2px var(--ctp-text);
    }

    progress::-webkit-progress-bar {
        background: transparent;
        border-radius: 3px;
    }

    progress:indeterminate {
        animation: progress-indeterminate 2s infinite linear;
        background: linear-gradient(90deg, var(--ctp-flamingo) 25%, var(--ctp-maroon) 50%, var(--ctp-flamingo) 75%);
        background-size: 200% 100%;
    }

    progress::-webkit-progress-value {
        background: var(--ctp-maroon);
        border-radius: 3px;
    }

    progress::-moz-progress-bar {
        background: var(--ctp-maroon);
        border-radius: 3px;
    }

    progress:indeterminate::-moz-progress-bar {
        background: transparent;
    }

    @keyframes progress-indeterminate {
        0% {
            background-position: 200% 0;
        }
        100% {
            background-position: -200% 0;
        }
    }
</style>
