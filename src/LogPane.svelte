<script lang="ts">
  import { onMount } from "svelte";
  import type { LogPage } from "./messages/LogPage.js";
  import type { LogRow } from "./messages/LogRow.js";
  import { query, delay } from "./ipc.js";
  import { repoStatusEvent, revisionSelectEvent } from "./stores.js";
  import Pane from "./Pane.svelte";
  import {
    type EnhancedRow,
    default as GraphLog,
    type EnhancedLine,
  } from "./GraphLog.svelte";
  import RevisionSummary from "./RevisionSummary.svelte";

  export let default_query: string;
  export let latest_query: string;

  const presets = [
    { label: "Default", query: default_query },
    { label: "Tracked Branches", query: "@ | ancestors(branches(), 5)" },
    { label: "Remote Branches", query: "@ | ancestors(remote_branches(), 5)" },
    { label: "All Revisions", query: "all()" },
  ];

  let choices: ReturnType<typeof getChoices>;
  let entered_query = latest_query;
  let graphRows: EnhancedRow[] | undefined;

  let log: HTMLElement;
  let logHeight = 0;
  let logScrollTop = 0;
  let pollFrame;

  $: if (entered_query) choices = getChoices();
  $: if ($repoStatusEvent) reloadLog();

  function getChoices() {
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

  async function loadLog() {
    let fetch = query<LogPage>("query_log", {
      revset: entered_query == "" ? "all()" : entered_query,
    });

    let page = await Promise.race([fetch, delay<LogPage>(200)]);

    if (page.type == "wait") {
      graphRows = undefined;
      page = await fetch;
    }

    if (page.type == "data") {
      graphRows = [];
      graphRows = addPageToGraph(graphRows, page.value.rows);

      if (page.value.rows.length > 0) {
        $revisionSelectEvent = page.value.rows[0].revision;
      }

      while (page.value.has_more) {
        let next_page = await query<LogPage>("query_log_next_page");
        if (next_page.type == "data") {
          graphRows = addPageToGraph(graphRows, next_page.value.rows);
          page = next_page;
        } else {
          break;
        }
      }
    }
  }

  async function reloadLog() {
    let fetch = query<LogPage>("query_log", {
      revset: entered_query == "" ? "all()" : entered_query,
    });

    let page = await Promise.race([fetch, delay<LogPage>(200)]);

    if (page.type == "wait") {
      graphRows = undefined;
      page = await fetch;
    }

    if (page.type == "data") {
      graphRows = [];
      graphRows = addPageToGraph(graphRows, page.value.rows);

      while (page.value.has_more) {
        let next_page = await query<LogPage>("query_log_next_page");
        if (next_page.type == "data") {
          graphRows = addPageToGraph(graphRows, next_page.value.rows);
          page = next_page;
        } else {
          break;
        }
      }
    }
  }

  // augment rows with all lines that pass through them
  let lineKey = 0;
  let passNextRow: EnhancedLine[] = [];
  function addPageToGraph(graph: EnhancedRow[], page: LogRow[]): EnhancedRow[] {
    for (let row of page) {
      let enhancedRow = row as EnhancedRow;
      enhancedRow.passingLines = passNextRow;
      passNextRow = [];

      for (let line of enhancedRow.lines) {
        let enhancedLine = line as EnhancedLine;
        enhancedLine.key = lineKey++;

        if (line.type == "ToIntersection") {
          // ToIntersection lines begin at their owning row, so they run from this row to the next one that we read (which may not be on the same page)
          enhancedRow.passingLines.push(enhancedLine);
          passNextRow.push(enhancedLine);
        } else {
          // other lines end at their owning row, so we need to add them to all previous rows and then this one
          for (let i = line.source[1]; i < line.target[1]; i++) {
            graph[i].passingLines.push(enhancedLine);
          }
          enhancedRow.passingLines.push(enhancedLine);
        }
      }

      graph.push(enhancedRow);
    }

    return graph;
  }

  function pollScroll() {
    if (log && log.scrollTop !== logScrollTop) {
      logScrollTop = log.scrollTop;
    }

    pollFrame = requestAnimationFrame(pollScroll);
  }

  onMount(() => {
    loadLog();
    pollFrame = requestAnimationFrame(pollScroll);
  });
</script>

<Pane>
  <div slot="header" class="log-selector">
    <select bind:value={entered_query} on:change={reloadLog}>
      {#each choices as choice}
        <option selected={choice.selected} value={choice.query}
          >{choice.label}</option
        >
      {/each}
    </select>
    <input type="text" bind:value={entered_query} on:change={reloadLog} />
  </div>

  <div
    slot="body"
    class="log-commits"
    bind:this={log}
    bind:clientHeight={logHeight}
  >
    {#if graphRows}
      <GraphLog
        containerHeight={logHeight}
        scrollTop={logScrollTop}
        rows={graphRows}
        let:row
      >
        {#if row}
          <RevisionSummary
            revision={row.revision}
            selected={$revisionSelectEvent?.change_id.prefix ==
              row.revision.change_id.prefix}
          />
        {/if}
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
