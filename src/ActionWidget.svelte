<script lang="ts">
    import { currentMutation } from "./stores";

    export let safe: boolean = false;
    export let disabled: boolean = false;
    export let onClick: (event: MouseEvent) => void;
</script>

{#if disabled || $currentMutation}
    <button disabled class:safe>
        <slot />
    </button>
{:else}
    <button on:click={onClick} class:safe>
        <slot />
    </button>
{/if}

<style>
    button {
        height: 24px;
        font-size: 16px;

        background: var(--ctp-flamingo);
        border-width: 1px;
        border-radius: 3px;
        border-color: var(--ctp-overlay0);
        box-shadow: 2px 2px var(--ctp-overlay0);

        font-family: var(--stack-industrial);
        display: flex;
        align-items: center;
        gap: 3px;

        padding: 1px 6px;
    }

    button:not(:disabled) {
        &:hover {
            background: var(--ctp-maroon);
        }
        &:active {
            padding: 2px 5px 0px 7px;
        }
    }

    button.safe {
        background: var(--ctp-green);
        &:hover {
            background: var(--ctp-teal);
        }
    }

    button:disabled {
        background: var(--ctp-mantle);
        color: var(--ctp-subtext0);
    }
</style>
