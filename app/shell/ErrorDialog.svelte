<script lang="ts">
    import ActionWidget from "../controls/ActionWidget.svelte";
    import ModalDialog from "./ModalDialog.svelte";

    export let title: string;
    export let severe: boolean = false;
    export let onClose: (() => void) | null = null;
</script>

<ModalDialog {title} error={severe} on:cancel={() => onClose?.()}>
    <div class="error-content">
        <slot />
    </div>

    <svelte:fragment slot="commands">
        {#if onClose}
            <ActionWidget tip="close dialog" safe onClick={onClose}>OK</ActionWidget>
        {/if}
    </svelte:fragment>
</ModalDialog>

<style>
    .error-content {
        grid-column: 1/3;
    }
</style>