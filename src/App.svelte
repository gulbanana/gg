<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { call, init } from "./ipc.js";
  import type { RevHeader } from "./messages/RevHeader.js";
  import type { RevDetail } from "./messages/RevDetail.js";
  import type { RevId } from "./messages/RevId.js";
  import Bound from "./Bound.svelte";
  import IdSpan from "./IdSpan.svelte";
  import Icon from "./Icon.svelte";
  import Pane from "./Pane.svelte";

  let log_content = init<RevHeader[]>();
  let change_content = init<RevDetail>();

  async function load_log() {
    log_content = await call<RevHeader[]>("load_log");
    change_content = init();
    if (log_content.type == "data") {
      await load_change(log_content.value[0].change_id);
    }
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
  <Pane>
    <div slot="header" class="log-selector">
      <select>
        <option selected>revsets.log</option>
        <option>all()</option>
      </select>
      <input
        type="text"
        value="@ | ancestors(immutable_heads().., 2) | heads(immutable_heads())"
      />
    </div>
    <div slot="body" class="log-commits">
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
  </Pane>

  <Bound ipc={change_content} let:value>
    <Pane>
      <h2 slot="header">
        <IdSpan id={value.header.change_id} type="change" />
        <button class="pin-commit"><Icon name="map-pin" /> Pin</button>
      </h2>

      <div slot="body">
        <textarea>{value.header.description.lines.join("\n")}</textarea>
        {#each value.paths as path}
          <div>{path.relative_path}</div>
        {/each}
      </div>
    </Pane>
  </Bound>

  <div id="status-bar">
    <span>C:\Users\user\repository</span>
    <button><Icon name="rotate-ccw" /> Undo</button>
  </div>
</div>

<style>
  #shell {
    width: 100vw;
    height: 100vh;

    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 26px;
    gap: 3px;

    background: var(--ctp-overlay0);
    color: var(--ctp-text);
  }

  .log-selector {
    height: 100%;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px;
  }

  .log-commits {
    overflow-x: hidden;
    overflow-y: scroll;
    scrollbar-color: var(--ctp-text) var(--ctp-base);
    display: flex;
    flex-direction: column;
    gap: 1em;
    user-select: none;
  }

  #status-bar {
    grid-column: 1/3;
    background: var(--ctp-crust);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 3px;
  }

  .change {
    display: flex;
    flex-direction: column;
    cursor: pointer;
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

  h2 {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
</style>
