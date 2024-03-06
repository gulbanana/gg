<script lang="ts">
    import type { RevId } from "./messages/RevId";
    export let id: RevId;
    export let type: "change" | "commit";
    export let context: boolean = false;

    let suffix = id.rest.substring(0, 8 - id.prefix.length);
</script>

<span class="id" class:context>
    <span class={type}>{id.prefix}</span>{suffix}
</span>

<style>
    .id {
        user-select: text;
        color: var(--ctp-subtext1);
        font-family: var(--stack-code);
    }

    .change {
        color: var(--ctp-pink);
    }

    .commit {
        color: var(--ctp-mauve);
    }

    .context {
        color: var(--ctp-rosewater);
    }
</style>
