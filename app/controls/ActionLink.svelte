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

        color: var(--gg-colors-success);
        font-family: var(--gg-text-familyDisplay);
        font-size: var(--gg-text-sizeLg);

        display: flex;
        align-items: center;
        cursor: pointer;
    }

    button:not(:disabled) {
        &:hover {
            color: var(--gg-colors-success);
        }
        &:focus-visible {
            text-decoration: underline;
            outline: solid 2px var(--gg-colors-focusRing);
            border-radius: 3px;
        }
        &:active {
            text-decoration: underline;
        }
    }

    button:disabled {
        background: var(--gg-colors-surface);
        color: var(--gg-colors-foregroundSubtle);
    }
</style>
