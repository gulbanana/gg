<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { LogChange } from "./messages.js";

  let log_content: LogChange[] = [];

  async function load() {
    log_content = await invoke<LogChange[]>("load_log");
  }

  listen("gg://repo_loaded", load);
  load();
</script>

<div class="commits">
  {#each log_content as change}
    <p>
      <code>
        {change.change_id}
        {change.email}
        {change.timestamp}
        {change.commit_id}
      </code><br />
      {change.description}
    </p>
  {/each}
</div>

<style>
  .commits {
    border: 2px solid red;
  }

  .commits > p:nth-child(even) {
    background: white;
    color: black;
  }
</style>
