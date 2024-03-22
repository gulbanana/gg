<script lang="ts">
    import { dragOverWidget } from "../stores";

    export let id: string | null = null;
    export let checked: boolean;
</script>

<label>
    <input {id} type="checkbox" bind:checked on:dragenter={dragOverWidget} on:dragover={dragOverWidget} />
    <slot />
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
