<script lang="ts" generics="T extends {value: string, separator?: boolean}">
    import { createEventDispatcher } from "svelte";
    import Icon from "./Icon.svelte";

    interface $$Slots {
        default: { option: T };
    }

    interface $$Events {
        change: CustomEvent<Event>;
    }

    export let id: string | null = null;
    export let options: T[];
    export let value: string;

    let dispatch = createEventDispatcher();
</script>

<select {id} bind:value on:change={(event) => dispatch("change", event)}>
    {#each options as option}
        {#if option.separator}
            <hr class="separator" />
        {:else}
            <option selected={value == option.value} value={option.value}>
                <slot {option}>{option.value}</slot>
            </option>
        {/if}
    {/each}
</select>

<style>
    select,
    ::picker(select) {
        appearance: base-select;
    }

    select {
        height: 30px;
        min-width: 150px;
        padding-left: 3px;
        padding-right: 3px;

        display: flex;
        align-items: center;

        cursor: pointer;

        &:focus-visible {
            padding-left: 2px;
        }
    }

    ::picker(select) {
        margin-top: 3px;
        padding-bottom: 6px;
    }

    select::picker-icon {
        content: "";
        display: block;
        width: 6px;
        height: 6px;
        border-right: 2px solid currentColor;
        border-bottom: 2px solid currentColor;
        transform: rotate(45deg);
        margin-top: -2px;
        margin-right: 4px;
    }

    select:open::picker-icon {
        transform: rotate(225deg);
        margin-top: 4px;
    }

    option {
        outline: none;
    }

    .separator {
        border-color: var(--ctp-overlay0);
        margin: 6px 0;
    }
</style>
