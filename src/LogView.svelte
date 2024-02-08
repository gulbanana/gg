<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { LogChange } from "./messages.js";

  let log_content: LogChange[] = [];

  async function load() {
    log_content = await invoke<LogChange[]>("load_log");
  }

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
