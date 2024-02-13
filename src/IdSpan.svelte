<script lang="ts">
    import type { RevId } from "./messages/RevId";
    export let id: RevId;
    export let type: "change" | "commit";
</script>

<span class="id"><span class={type}>{id.prefix}</span>{id.rest}</span>

<style>
    .id {
        color: var(--ctp-subtext1);
    }
    .change {
        color: var(--ctp-pink);
    }
    .commit {
        color: var(--ctp-mauve);
    }
</style>
