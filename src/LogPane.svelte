<script lang="ts">
    import type { LogPage } from "./messages/LogPage.js";
    import type { RevHeader } from "./messages/RevHeader.js";
    import { command, event } from "./ipc.js";
    import Bound from "./Bound.svelte";
    import Pane from "./Pane.svelte";
    import Graph from "./Graph.svelte";

    export let query: string;

    const log_content = command<LogPage>("query_log");
    const change_content = event<RevHeader>("gg://change/select");

    let entered_query = query;

    load_log();

    async function load_log() {
        let log = await log_content.call({
            revset: entered_query == "" ? "all()" : entered_query,
        });

        if (log.type == "data" && log.value.nodes.length > 0) {
            $change_content = log.value.nodes[0].revision;
        }
    }
</script>

<Pane>
    <div slot="header" class="log-selector">
        <select>
            <option selected>revsets.log</option>
            <option>all()</option>
        </select>
        <input type="text" bind:value={entered_query} on:change={load_log} />
    </div>

    <div slot="body" class="log-commits">
        <Bound query={$log_content} let:data>
            <div slot="wait">Loading changes...</div>
            <Graph page={data} />
        </Bound>
    </div>
</Pane>

<style>
    .log-selector {
        height: 100%;
        display: grid;
        grid-template-columns: auto 1fr;
        gap: 3px;
    }

    input {
        font-family: var(--stack-code);
        font-size: 14px;
    }

    .log-commits {
        overflow-x: hidden;
        overflow-y: scroll;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);
        display: grid;
        user-select: none;
    }
</style>
