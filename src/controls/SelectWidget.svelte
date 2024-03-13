<script lang="ts" generics="T extends {selected: boolean, value: string}">
    import { createEventDispatcher } from "svelte";
    import Icon from "./Icon.svelte";

    interface $$Slots {
        default: { option: T };
    }

    interface $$Events {
        change: CustomEvent<Event>;
    }

    export let options: T[];
    export let value: string;

    let dispatch = createEventDispatcher();
</script>

<div class="wrapper">
    <select bind:value on:change={(event) => dispatch("change", event)}>
        {#each options as option}
            <option selected={option.selected} value={option.value}>
                <slot {option} />
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