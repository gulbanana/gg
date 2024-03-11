<script lang="ts">
    export let checked: boolean;
</script>

<label>
    <input type="checkbox" bind:checked />
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
