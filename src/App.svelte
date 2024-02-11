<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { call, init } from "./ipc.js";
  import type { RevHeader } from "./messages/RevHeader.js";
  import type { RevDetail } from "./messages/RevDetail.js";
  import type { RevId } from "./messages/RevId.js";
  import Bound from "./Bound.svelte";
  import IdSpan from "./IdSpan.svelte";

  let log_content = init<RevHeader[]>();
  let change_content = init<RevDetail>();

  async function load_log() {
    log_content = await call<RevHeader[]>("load_log");
    change_content = init();
  }

  async function load_change(id: RevId) {
    change_content = await call<RevDetail>("load_change", {
      revision: id.prefix + id.rest,
    });
  }

  listen("gg://repo_loaded", load_log);
  load_log();

  document.addEventListener("keydown", (event) => {
    if (event.key === "o" && event.ctrlKey) {
      event.preventDefault();
      invoke("forward_accelerator", { key: "o" });
    }
  });

  invoke("notify_window_ready");
</script>

<div id="shell">
  <div id="log" class="pane">
    <Bound ipc={log_content} let:value>
      {#each value as change}
        <!-- svelte-ignore a11y-click-events-have-key-events -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <div class="change" on:click={() => load_change(change.change_id)}>
          <code class="change-line">
            <IdSpan id={change.change_id} type="change" />
            <span class="author">{change.email}</span>
            <span class="timestamp">{change.timestamp}</span>
            <IdSpan id={change.commit_id} type="commit" />
          </code>
          <span class="change-line">
            {change.description.lines[0]}
          </span>
        </div>
      {/each}
    </Bound>
  </div>

  <div id="selected-change" class="pane">
    <Bound ipc={change_content} let:value>
      <textarea>foo bar</textarea>
      {#each value.header.description.lines as line}
        <div>{line}</div>
      {/each}
      {#each value.paths as path}
        <p>{path.relative_path}</p>
      {/each}
    </Bound>
  </div>

  <div id="status-bar">
    <span>C:\Users\user\repository</span>
    <button>Undo!</button>
  </div>
</div>

<style>
  #shell {
    width: 100vw;
    height: 100vh;

    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 2em;
    gap: 10px;

    background: var(--ctp-base);
    color: var(--ctp-text);
  }

  #log {
    overflow-x: hidden;
    overflow-y: scroll;
    scrollbar-color: var(--ctp-text) var(--ctp-mantle);
    border-width: 1px 1px 1px 0;
    display: flex;
    flex-direction: column;
    gap: 1em;
  }

  #selected-change {
    border-width: 1px 0 1px 1px;
  }

  #status-bar {
    grid-column: 1/3;
    background: var(--ctp-crust);
    border-top: 1px solid var(--ctp-overlay0);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 5px;
  }

  #status-bar > button {
    background: var(--ctp-peach);
    border-width: 1px;
    border-radius: 2px;
    border-color: var(--ctp-overlay0);
    &:active,
    &:hover {
      border-color: var(--ctp-lavender);
    }
  }

  .pane {
    background: var(--ctp-mantle);
    border: solid var(--ctp-overlay0);
    margin-top: 10px;
    padding: 5px;
  }

  .change {
    display: flex;
    flex-direction: column;
  }

  .change-line {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .author {
    color: var(--ctp-yellow);
    display: inline-block;
    width: 24ch;
  }

  .timestamp {
    color: var(--ctp-teal);
  }

  textarea {
    width: 100%;
    caret-color: var(--ctp-rosewater);
    outline: none;
    border-color: var(--ctp-overlay0);
    &:focus-visible {
      border-color: var(--ctp-lavender);
    }
  }

  ::selection {
    background: var(--ctp-rosewater);
  }
</style>
