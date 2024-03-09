<script lang="ts">
    import type { RevId } from "./messages/RevId";
    export let id: RevId;
    export let type: "change" | "commit";

    let suffix = id.rest.substring(0, 8 - id.prefix.length);
</script>

<span class="idd"><span class={type}>{id.prefix}</span>{suffix}</span>

<style>
    .id {
        user-select: text;
        color: var(--ctp-subtext1);
    }

    .change {
        color: var(--ctp-pink);
    }

    .commit {
        color: var(--ctp-mauve);
    }
</style>
