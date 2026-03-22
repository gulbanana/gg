<script lang="ts">
    import { onMount } from "svelte";
    import type { OpLog } from "./messages/OpLog";
    import type { OpLogEntry } from "./messages/OpLogEntry";
    import { query } from "./ipc";
    import { repoStatusEvent, revisionSelectEvent, selectedOpId } from "./stores";
    import Icon from "./controls/Icon.svelte";

    const ROW_HEIGHT = 26;
    const LOAD_THRESHOLD = ROW_HEIGHT * 5; // load more when within 5 rows of bottom

    let entries: OpLogEntry[] = [];
    let hasMore = false;
    let loading = false;
    let expanded = false;
    let filterSnapshots = true;

    let scrollEl: HTMLElement;
    let viewportHeight = 0;
    let scrollTop = 0;

    $: startIndex = Math.floor(scrollTop / ROW_HEIGHT);
    $: visibleCount = Math.ceil(viewportHeight / ROW_HEIGHT) + 2;
    $: endIndex = Math.min(startIndex + visibleCount, entries.length);
    $: visibleSlice = entries.slice(startIndex, endIndex);
    $: totalHeight = entries.length * ROW_HEIGHT;

    async function loadOpLog(afterId?: string) {
        if (loading) return;
        loading = true;
        let result = await query<OpLog>("query_op_log", { filterSnapshots, afterId });
        loading = false;
        if (result.type === "data") {
            if (afterId) {
                entries = [...entries, ...result.value.entries];
            } else {
                entries = result.value.entries;
            }
            hasMore = result.value.has_more;
        } else {
            console.error("query_op_log failed:", result);
        }
    }

    function onScroll() {
        if (!scrollEl) return;
        scrollTop = scrollEl.scrollTop;
        viewportHeight = scrollEl.clientHeight;
        if (hasMore && !loading) {
            let distanceFromBottom = totalHeight - (scrollTop + viewportHeight);
            if (distanceFromBottom < LOAD_THRESHOLD && entries.length > 0) {
                loadOpLog(entries[entries.length - 1].id);
            }
        }
    }

    onMount(() => {
        loadOpLog();
        if (scrollEl) viewportHeight = scrollEl.clientHeight;
    });
    $: if ($repoStatusEvent) loadOpLog();
    $: if ($revisionSelectEvent) $selectedOpId = null;
    $: filterSnapshots, loadOpLog();

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
    <div class="op-log-header">
        <button class="op-log-expand" on:click={() => (expanded = !expanded)}>
            <Icon name={expanded ? "chevron-down" : "chevron-right"} />
            <span>Operations ({entries.length}{hasMore ? "+" : ""})</span>
        </button>
        <button
            class="filter-btn"
            class:active={filterSnapshots}
            title={filterSnapshots ? "Click to show snapshot operations" : "Click to hide snapshot operations"}
            on:click={() => (filterSnapshots = !filterSnapshots)}
        >
            <Icon name="filter" />
        </button>
    </div>
    {#if expanded}
        <div
            class="op-log-body"
            bind:this={scrollEl}
            bind:clientHeight={viewportHeight}
            on:scroll={onScroll}
        >
            <div class="op-log-spacer" style="height: {totalHeight}px">
                {#each visibleSlice as entry, i (i)}
                    <button
                        class="op-entry"
                        class:head={entry.is_head}
                        class:selected={$selectedOpId === entry.id}
                        style="top: {(startIndex + i) * ROW_HEIGHT}px"
                        on:click={() => $selectedOpId = $selectedOpId === entry.id ? null : entry.id}
                    >
                        <span class="op-time" title={entry.timestamp}>{formatTime(entry.timestamp)}</span>
                        <span class="op-desc">{entry.description || "(no description)"}</span>
                        <span class="op-id" title={entry.id}>{entry.id.slice(0, 8)}</span>
                    </button>
                {/each}
            </div>
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
        height: 28px;
        background: var(--ctp-mantle);
    }

    .op-log-expand {
        flex: 1;
        display: flex;
        align-items: center;
        gap: 3px;
        padding: 3px 6px;
        height: 100%;
        border: none;
        background: none;
        color: var(--ctp-text);
        cursor: pointer;
        font-family: var(--stack-industrial);
        font-size: 13px;
        text-align: left;

        &:hover {
            background: var(--ctp-surface0);
        }
    }

    .filter-btn {
        margin-left: auto;
        display: flex;
        align-items: center;
        padding: 2px 4px;
        border: none;
        background: none;
        color: var(--ctp-overlay1);
        cursor: pointer;
        border-radius: 3px;

        &:hover {
            background: var(--ctp-surface1);
            color: var(--ctp-text);
        }

        &.active {
            color: var(--ctp-blue);
        }
    }

    .op-log-body {
        max-height: 200px;
        overflow-y: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);

        &::-webkit-scrollbar {
            width: 6px;
        }

        &::-webkit-scrollbar-thumb {
            background-color: var(--ctp-text);
            border-radius: 6px;
        }

        &::-webkit-scrollbar-track {
            background-color: var(--ctp-crust);
        }
    }

    .op-log-spacer {
        position: relative;
    }

    .op-entry {
        position: absolute;
        left: 0;
        right: 0;
        height: 26px;
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
        align-self: center;
    }

    .op-desc {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        align-self: center;
    }

    .op-id {
        color: var(--ctp-overlay1);
        font-family: var(--stack-code);
        font-size: 11px;
        align-self: center;
    }
</style>
