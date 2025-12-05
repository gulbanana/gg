<script lang="ts">
    import { hasModal } from "../stores";
    import type { Snippet } from "svelte";

    let { header, body }: { header?: Snippet; body?: Snippet } = $props();
</script>

<section inert={$hasModal}>
    <div class="header">
        {@render header?.()}
    </div>
    <div class="body">
        {@render body?.()}
    </div>
</section>

<style>
    section {
        display: grid;
        grid-template-rows: 40px 1fr;
        grid-template-columns: 100%;
        overflow: hidden;
    }

    .header {
        border-bottom: 1px solid var(--ctp-overlay0);
        padding: 6px 6px 3px 6px;
    }

    .body {
        padding: 3px 6px 6px 6px;
        display: grid;
        overflow: hidden;
    }
</style>
