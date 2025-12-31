<script lang="ts">
    import CheckWidget from "./CheckWidget.svelte";

    export let id: string | null = null;
    export let checked: boolean;
</script>

<label>
    <CheckWidget {id} bind:checked />
    <slot />
</label>

<style>
    label {
        display: flex;
        align-items: center;
        font-family: var(--stack-industrial);
        color: var(--ctp-text);
        gap: 3px;

        cursor: pointer;
    }
</style>
