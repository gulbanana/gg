<!--
@component
Abstraction of dubious utility - it's only used in one place, because most IPC does not follow a query-like pattern.
-->

<script lang="ts" generics="T">
    import type { Query as Query } from "../ipc";

    interface $$Slots {
        wait: {};
        error: { message: string };
        default: { data: T };
    }

    export let query: Query<T>;

    let type = query.type;
</script>

{#key query}
    {#if query.type == "wait"}
        <slot name="wait" />
    {:else if query.type == "error"}
        <slot name="error" message={query.message}>
            <span class="red">{query.message}</span>
        </slot>
    {:else}
        <slot data={query.value} />
    {/if}
{/key}

<style>
    .red {
        color: #d20f39;
    }
</style>
