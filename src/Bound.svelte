<script lang="ts" generics="T">
    import type { Query as Query } from "./ipc";

    export let query: Query<T>;
    interface $$Slots {
        wait: {};
        error: { message: string };
        default: { data: T };
    }

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
