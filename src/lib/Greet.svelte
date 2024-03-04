<script lang="ts">
  import { invoke } from "@tauri-apps/api/core"

  let name = "";
  let greeting = "";

  async function greet(){
    greeting = await invoke("greet", { name })
  }
</script>

<div>
  <form on:submit|preventDefault={greet}>
    <input placeholder="Enter a name..." bind:value={name} />
    <button type="submit">Helloworld me</button>
  </form>
  <p>{greeting}</p>
</div>