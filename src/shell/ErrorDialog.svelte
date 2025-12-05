<script lang="ts">
    import ActionWidget from "../controls/ActionWidget.svelte";
    import ModalDialog from "./ModalDialog.svelte";
    import type { Snippet } from "svelte";

    let { title, severe = false, onClose = null, children }: {
        title: string;
        severe?: boolean;
        onClose?: (() => void) | null;
        children?: Snippet;
    } = $props();
</script>

<ModalDialog {title} error={severe} oncancel={onClose ?? undefined}>
    {@render children?.()}

    {#snippet commands()}
        {#if onClose}
            <ActionWidget tip="close dialog" safe onClick={onClose}>OK</ActionWidget>
        {/if}
    {/snippet}
</ModalDialog>
