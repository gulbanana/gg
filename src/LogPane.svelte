<script lang="ts">
  import type { LogPage } from "./messages/LogPage.js";
  import type { LogRow } from "./messages/LogRow.js";
  import type { RevHeader } from "./messages/RevHeader.js";
  import { call, event } from "./ipc.js";
  import Pane from "./Pane.svelte";
  import Graph from "./Graph.svelte";

  export let query: string;

  const change_content = event<RevHeader>("gg://change/select");

  let entered_query = query;
  let log_rows: LogRow[] | undefined;

  load_log();

  async function load_log() {
    log_rows = undefined;

    let page = await call<LogPage>("query_log", {
      revset: entered_query == "" ? "all()" : entered_query,
    });

    if (page.type == "data") {
      if (page.value.rows.length > 0) {
        $change_content = page.value.rows[0].revision;
      }

      log_rows = page.value.rows;

      while (page.value.has_more) {
        let next_page = await call<LogPage>("query_log_more");
        if (next_page.type == "data") {
          log_rows = log_rows?.concat(next_page.value.rows);
          page = next_page;
        } else {
          break;
        }
      }
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
    {#if log_rows}
      <Graph rows={log_rows} />
    {:else}
      <div>Loading changes...</div>
    {/if}
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
