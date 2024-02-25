<script lang="ts">
  import { onMount } from "svelte";
  import type { LogPage } from "./messages/LogPage.js";
  import type { LogRow } from "./messages/LogRow.js";
  import { query, delay } from "./ipc.js";
  import { revisionSelectEvent } from "./stores.js";
  import Pane from "./Pane.svelte";
  import GraphLog from "./GraphLog.svelte";
  import RevisionSummary from "./RevisionSummary.svelte";

  export let default_query: string;
  export let latest_query: string;

  const presets = [
    { label: "Default", query: default_query },
    { label: "Tracked Branches", query: "@ | ancestors(branches(), 5)" },
    { label: "Remote Branches", query: "@ | ancestors(remote_branches(), 5)" },
    { label: "All Revisions", query: "all()" },
  ];

  let choices: ReturnType<typeof get_choices>;
  $: if (entered_query) choices = get_choices();

  let entered_query = latest_query;
  let log_rows: LogRow[] | undefined;

  onMount(load_log);

  function get_choices() {
    let choices = presets.map((p) => ({ ...p, selected: false }));
    for (let choice of choices) {
      if (entered_query == choice.query) {
        choice.selected = true;
        return choices;
      }
    }

    choices = [
      { label: "Custom", query: entered_query, selected: true },
      ...choices,
    ];

    return choices;
  }

  async function load_log() {
    let fetch = query<LogPage>("query_log", {
      revset: entered_query == "" ? "all()" : entered_query,
    });

    let page = await Promise.race([fetch, delay<LogPage>(200)]);

    if (page.type == "wait") {
      log_rows = undefined;
      page = await fetch;
    }

    if (page.type == "data") {
      log_rows = page.value.rows;

      if (page.value.rows.length > 0) {
        $revisionSelectEvent = page.value.rows[0].revision;
      }

      while (page.value.has_more) {
        let next_page = await query<LogPage>("query_log_next_page");
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
    <select bind:value={entered_query} on:change={load_log}>
      {#each choices as choice}
        <option selected={choice.selected} value={choice.query}
          >{choice.label}</option
        >
      {/each}
    </select>
    <input type="text" bind:value={entered_query} on:change={load_log} />
  </div>

  <div slot="body" class="log-commits">
    {#if log_rows}
      <GraphLog rows={log_rows} let:row>
        <RevisionSummary
          revision={row.revision}
          selected={$revisionSelectEvent?.change_id.prefix ==
            row.revision.change_id.prefix}
        />
      </GraphLog>
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
