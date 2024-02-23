<button>
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
            border-color: var(--ctp-lavender);
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
</style>
