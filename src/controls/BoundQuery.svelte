<!--
@component
Abstraction of dubious utility - it's only used in one place, because most IPC does not follow a query-like pattern.
-->

<script lang="ts" generics="T">
    import type { Query as Query } from "../ipc";
    import type { Snippet } from "svelte";

    let { query, wait, error, children }: {
        query: Query<T>;
        wait?: Snippet;
        error?: Snippet<[string]>;
        children: Snippet<[T]>;
    } = $props();
</script>

{#key query}
    {#if query.type == "wait"}
        {#if wait}
            {@render wait()}
        {/if}
    {:else if query.type == "error"}
        {#if error}
            {@render error(query.message)}
        {:else}
            <span class="red">{query.message}</span>
        {/if}
    {:else}
        {@render children(query.value)}
    {/if}
{/key}

<style>
    .red {
        color: #d20f39;
    }
</style>
