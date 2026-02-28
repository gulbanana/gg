<script lang="ts">
    import { dragOverWidget } from "../stores";

    export let tip: string = "";
    export let onClick: (event: MouseEvent) => void;
</script>

<button on:click|stopPropagation={onClick} on:dragenter={dragOverWidget} on:dragover={dragOverWidget} title={tip}>
    <slot />
</button>

<style>
    button {
        height: 24px;
        padding: 0;
        margin: 0;
        padding: 1px;

        background: none;
        border: none;

        color: var(--ctp-sapphire);
        font-family: var(--stack-industrial);
        font-size: 16px;

        display: flex;
        align-items: center;
        cursor: pointer;
    }

    button:not(:disabled) {
        &:hover {
            color: var(--ctp-teal);
        }
        &:focus-visible {
            text-decoration: underline;
            outline: solid 2px var(--ctp-lavender);
            border-radius: 3px;
        }
        &:active {
            text-decoration: underline;
        }
    }

    button:disabled {
        background: var(--ctp-mantle);
        color: var(--ctp-overlay2);
    }
</style>
