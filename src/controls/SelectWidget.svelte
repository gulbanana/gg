<script lang="ts" generics="T extends {value: string}">
    import Icon from "./Icon.svelte";
    import type { Snippet } from "svelte";

    let { id = null, options, value = $bindable(), onchange, children }: {
        id?: string | null;
        options: T[];
        value: string;
        onchange?: (event: Event) => void;
        children?: Snippet<[T]>;
    } = $props();
</script>

<div class="wrapper">
    <select {id} bind:value onchange={(event) => onchange?.(event)}>
        {#each options as option}
            <option selected={value == option.value} value={option.value}>
                {#if children}
                    {@render children(option)}
                {:else}
                    {option.value}
                {/if}
            </option>
        {/each}
    </select>
    <Icon name="chevron-down" />
</div>

<style>
    select {
        appearance: none;
        padding-left: 3px;

        &:focus-visible {
            padding-left: 2px;
        }
    }

    .wrapper {
        display: flex;
        position: relative;
    }

    .wrapper > :global(:last-child) {
        position: absolute;
        right: 0;
        height: 32px;
        right: 3px;
    }
</style>