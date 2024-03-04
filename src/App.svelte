<script lang="ts">
  import LogView from './lib/LogView.svelte'
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