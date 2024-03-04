<script lang="ts">
  import { invoke } from "@tauri-apps/api/core"

  let log_content = "";

  async function load() {
    log_content = await invoke("load_log")
  }
</script>

<div>
  <form on:submit|preventDefault={load}>
    <button type="submit">Load Log</button>
  </form>
  <pre>{log_content}</pre>
</div>