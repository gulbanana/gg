<script lang="ts">
    import { onMount } from "svelte";
    import type { OpLog } from "./messages/OpLog";
    import type { OpLogEntry } from "./messages/OpLogEntry";
    import { query } from "./ipc";
    import { repoStatusEvent, revisionSelectEvent, selectedOpId } from "./stores";
    import Icon from "./controls/Icon.svelte";
    import ActionWidget from "./controls/ActionWidget.svelte";

    let entries: OpLogEntry[] = [];
    let expanded = false;


    async function loadOpLog() {
        let result = await query<OpLog>("query_op_log", { maxCount: 50 });
        if (result.type === "data") {
            entries = result.value.entries;
        }
    }

    onMount(loadOpLog);
    $: if ($repoStatusEvent) loadOpLog();
    $: if ($revisionSelectEvent) $selectedOpId = null;

    function formatTime(timestamp: string): string {
        let date = new Date(timestamp);
        let now = new Date();
        let diffMs = now.getTime() - date.getTime();
        let diffMins = Math.floor(diffMs / 60000);
        if (diffMins < 1) return "just now";
        if (diffMins < 60) return `${diffMins}m ago`;
        let diffHours = Math.floor(diffMins / 60);
        if (diffHours < 24) return `${diffHours}h ago`;
        let diffDays = Math.floor(diffHours / 24);
        return `${diffDays}d ago`;
    }
</script>

<div class="op-log">
    <button class="op-log-header" on:click={() => (expanded = !expanded)}>
        <Icon name={expanded ? "chevron-down" : "chevron-right"} />
        <span>Operations ({entries.length})</span>
    </button>
    {#if expanded}
        <div class="op-log-body">
            {#each entries as entry}
                <button
                    class="op-entry"
                    class:head={entry.is_head}
                    class:selected={$selectedOpId === entry.id}
                    on:click={() => $selectedOpId = $selectedOpId === entry.id ? null : entry.id}
                >
                    <span class="op-time" title={entry.timestamp}>{formatTime(entry.timestamp)}</span>
                    <span class="op-desc">{entry.description || "(no description)"}</span>
                    <span class="op-id" title={entry.id}>{entry.id.slice(0, 8)}</span>
                </button>
            {/each}
        </div>
    {/if}
</div>

<style>
    .op-log {
        border-top: 1px solid var(--ctp-overlay0);
        flex-shrink: 0;
    }

    .op-log-header {
        width: 100%;
        display: flex;
        align-items: center;
        gap: 3px;
        padding: 3px 6px;
        height: 28px;
        border: none;
        background: var(--ctp-mantle);
        color: var(--ctp-text);
        cursor: pointer;
        font-family: var(--stack-industrial);
        font-size: 13px;

        &:hover {
            background: var(--ctp-surface0);
        }
    }

    .op-log-body {
        max-height: 200px;
        overflow-y: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);
    }

    .op-entry {
        display: grid;
        grid-template-columns: 60px 1fr auto;
        gap: 6px;
        padding: 2px 6px;
        font-size: 12px;
        border: none;
        border-bottom: 1px solid var(--ctp-surface0);
        background: none;
        color: var(--ctp-text);
        font-family: var(--stack-industrial);
        text-align: left;
        width: 100%;
        cursor: pointer;

        &.head {
            background: var(--ctp-base);
        }

        &.selected {
            background: var(--ctp-surface1);
            outline: 1px solid var(--ctp-blue);
            outline-offset: -1px;
        }

        &:hover {
            background: var(--ctp-surface0);
        }
    }

    .op-time {
        color: var(--ctp-subtext0);
        white-space: nowrap;
    }

    .op-desc {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .op-id {
        color: var(--ctp-overlay1);
        font-family: var(--stack-code);
        font-size: 11px;
    }
</style>
