<script lang="ts">
    import { dragOverWidget } from "../stores";

    export let id: string | null = null;
    export let checked: boolean;
</script>

<input {id} type="checkbox" bind:checked on:dragenter={dragOverWidget} on:dragover={dragOverWidget} />

<style>
    input:focus-visible {
        outline: 1px solid var(--gg-colors-focusRing);
        outline-offset: 0;
    }
</style>
