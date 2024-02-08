<script lang="ts">
  import LogView from "./LogView.svelte";
  import { invoke } from "@tauri-apps/api/core";

  document.addEventListener("keydown", (event) => {
    if (event.key === "o" && event.ctrlKey) {
      event.preventDefault();
      invoke("forward_accelerator", { key: "o" });
    }
  });
</script>

<div id="shell">
  <LogView />
</div>

<style>
  #shell {
    width: 100vw;
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
