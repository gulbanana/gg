<div id="overlay">
    <slot />
</div>

<style>
    #overlay {
        z-index: 1;
        position: absolute;
        top: 0;
        right: 0;
        bottom: 33px;
        left: 0;

        background: rgb(var(--ctp-overlay1-rgb) / 40%);
        pointer-events: all;

        display: grid;
        grid-template-columns: 1fr auto 1fr;
        grid-template-rows: 1fr auto 2fr;
    }
</style>
