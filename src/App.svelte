<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { ChangePath } from "./messages/ChangePath.js";
  import type { LogChange } from "./messages/LogChange.js";
  import type { Id } from "./messages/Id.js";

  let log_content: LogChange[] = [];
  let change_content: ChangePath[] = [];

  async function load_log() {
    log_content = await invoke<LogChange[]>("load_log");
  }

  async function load_change(id: Id) {
    try {
      console.log(id);
      change_content = await invoke<ChangePath[]>("load_change", {
        revision: id.prefix + id.rest,
      });
    } catch (error: any) {
      change_content = [{ relative_path: { lines: [error.toString()] } }];
    }
  }

  listen("gg://repo_loaded", load_log);
  load_log();

  document.addEventListener("keydown", (event) => {
    if (event.key === "o" && event.ctrlKey) {
      event.preventDefault();
      invoke("forward_accelerator", { key: "o" });
    }
  });
</script>

<div id="shell">
  <div id="log">
    {#each log_content as change}
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
  </div>
  <div id="selected-change">
    {#each change_content as path}
      {#each path.relative_path.lines as line}<div>{line}</div>{/each}
    {/each}
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
