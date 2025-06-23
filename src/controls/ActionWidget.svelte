<script lang="ts">
    import { dragOverWidget, hasModal } from "../stores";

    export let tip: string = "";
    export let onClick: (event: MouseEvent) => void;
    export let safe: boolean = false;
    export let disabled: boolean = false;
</script>

{#if disabled || (!safe && $hasModal)}
    <button disabled class:safe on:dragenter={dragOverWidget} on:dragover={dragOverWidget}>
        <slot />
    </button>
{:else}
    <button
        class:safe
        on:click={onClick}
        on:dragenter={dragOverWidget}
        on:dragover={dragOverWidget}
        title={safe ? "" : tip}>
        <slot />
    </button>
{/if}

<style>
    button {
        height: 24px;
        font-size: 16px;

        outline: none;
        margin: 0;
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

    button :global(.id) {
        color: var(--ctp-crust) !important;
    }

    button :global(.id .ChangeId) {
        color: rgb(198, 35, 117);
        font-weight: bold;
    }

    button :global(.id .CommitId) {
        color: rgb(38, 107, 255);
        font-weight: bold;
    }

    button:not(:disabled) {
        &:hover {
            background: var(--ctp-maroon);
        }
        &:focus-visible {
            border-color: var(--ctp-lavender);
            border-width: 2px;
            padding: 0px 5px;
            text-decoration: underline;
        }
        &:active {
            margin: 1px 0px 0px 1px;
            padding: 1px 5px 0px 6px;
            box-shadow: 1px 1px var(--ctp-overlay0);
            &:focus-visible {
                padding: 1px 4px 0px 5px;
            }
        }
    }

    button.safe {
        background: var(--ctp-sapphire);
        &:hover {
            background: var(--ctp-teal);
        }
        &:active {
            border-right-color: var(--ctp-teal);
            border-bottom-color: var(--ctp-teal);
        }
    }

    button:disabled {
        background: var(--ctp-mantle);
        color: var(--ctp-subtext1);
    }
</style>
