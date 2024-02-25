<script lang="ts">
    export let safe: boolean = false;
    export let onClick: (event: MouseEvent) => void;
</script>

<button on:click={onClick} class:safe>
    <slot />
</button>

<style>
    button {
        height: 24px;
        font-size: 16px;

        background: var(--ctp-peach);
        border-width: 1px;
        border-radius: 3px;
        border-color: var(--ctp-overlay0);
        box-shadow: 2px 2px var(--ctp-overlay0);

        &:hover {
            background: var(--ctp-maroon);
        }

        font-family: var(--stack-industrial);
        display: flex;
        align-items: center;
        gap: 3px;

        padding: 1px 6px;

        &:active {
            padding: 2px 5px 0px 7px;
        }
    }

    .safe {
        background: var(--ctp-green);
        &:hover {
            background: var(--ctp-teal);
        }
    }
</style>
