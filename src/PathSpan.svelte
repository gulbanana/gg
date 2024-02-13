<script lang="ts">
    import Icon from "./Icon.svelte";
    import type { DiffPath } from "./messages/DiffPath";
    export let path: DiffPath;
</script>

<span class={path.type}>
    {#if path.type == "Added"}
        <Icon name="file-plus" />
    {:else if path.type == "Modified"}
        <Icon name="file" />
    {:else if path.type == "Deleted"}
        <Icon name="file-minus" />
    {/if}

    {path.relative_path}
</span>

<style>
    .Added {
        color: var(--ctp-green);
    }

    .Modified {
        color: var(--ctp-yellow);
    }

    .Deleted {
        color: var(--ctp-red);
    }
</style>
