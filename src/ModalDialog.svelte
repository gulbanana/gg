<script lang="ts">
    import ActionWidget from "./controls/ActionWidget.svelte";
    import Icon from "./controls/Icon.svelte";

    export let title: string;
    export let severe: boolean = false;
    export let onClose: (() => void) | null = null;
</script>

<div id="overlay-chrome">
    <h3 id="overlay-header" class:severe>{title}</h3>

    <div id="overlay-content">
        <slot />
    </div>

    {#if onClose}
        <ActionWidget tip="close dialog" safe onClick={onClose}>
            <Icon name="x" />
        </ActionWidget>
    {/if}
</div>

<style>
    #overlay-chrome {
        grid-area: 2/2/2/2;

        background: var(--ctp-mantle);
        border-radius: 9px;
        border: 3px solid var(--ctp-overlay1);

        display: grid;
        grid-template-columns: 30px 1fr 33px;
        grid-template-rows: 30px auto 15px;
    }

    #overlay-chrome > :global(button) {
        grid-area: 1/3/1/3;
        width: 30px;
        height: 30px;
        margin: 1px 3px 0 0;
    }

    #overlay-header {
        margin-top: 3px;
        padding: 0 15px;
        grid-area: 1/2/2/2;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    #overlay-content {
        grid-area: 2/2/2/2;
    }

    #overlay-content > :first-child {
        margin-top: 0;
    }

    #overlay-content > :last-child {
        margin-bottom: 0;
    }

    .severe {
        color: var(--ctp-red);
    }
</style>
