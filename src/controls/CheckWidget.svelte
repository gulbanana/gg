<script lang="ts">
    import { dragOverWidget } from "../stores";
    import type { Snippet } from "svelte";

    let { id = null, checked = $bindable(), children }: {
        id?: string | null;
        checked: boolean;
        children?: Snippet;
    } = $props();
</script>

<label>
    <input {id} type="checkbox" bind:checked ondragenter={dragOverWidget} ondragover={dragOverWidget} />
    {@render children?.()}
</label>

<style>
    label {
        display: flex;
        align-items: center;
        font-family: var(--stack-industrial);
        color: var(--ctp-text);
        gap: 3px;
    }

    input:focus-visible {
        outline: 1px solid var(--ctp-lavender);
        outline-offset: 0;
    }
</style>
