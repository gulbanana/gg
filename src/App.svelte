<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { call, init } from "./ipc.js";
  import type { RevHeader } from "./messages/RevHeader.js";
  import type { RevDetail } from "./messages/RevDetail.js";
  import type { RevId } from "./messages/RevId.js";
  import Bound from "./Bound.svelte";

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
  <div id="log">
    <Bound ipc={log_content} let:value>
      {#each value as change}
        <!-- svelte-ignore a11y-click-events-have-key-events -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <div class="change" on:click={() => load_change(change.change_id)}>
          <code class="change-line">
            <span><b>{change.change_id.prefix}</b>{change.change_id.rest}</span>
            {change.email}
            {change.timestamp}
            <span><b>{change.commit_id.prefix}</b>{change.commit_id.rest}</span>
          </code>
          <span class="change-line">
            {change.description.lines[0]}
          </span>
        </div>
      {/each}
    </Bound>
  </div>
  <div id="selected-change">
    <Bound ipc={change_content} let:value>
      {#each value.header.description.lines as line}
        <div>{line}</div>
      {/each}
      {#each value.paths as path}
        <p>{path.relative_path}</p>
      {/each}
    </Bound>
  </div>
</div>

<style>
  #shell {
    width: 100vw;
    height: 100vh;
    display: grid;
    grid-template-columns: 1fr 1fr;
    column-gap: 10px;
  }

  #log {
    border-right: 1px solid black;
    overflow-x: hidden;
    overflow-y: scroll;
  }

  #log > div:nth-child(even) {
    background: white;
    color: black;
  }

  #selected-change {
    border-left: 1px solid black;
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
</style>
