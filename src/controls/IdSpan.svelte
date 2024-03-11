<script lang="ts">
    import type { RevId } from "../messages/RevId";
    export let id: RevId;
    export let type: "change" | "commit";
    export let pronoun: boolean = false;

    let suffix = id.rest.substring(0, 8 - id.prefix.length);
</script>

<span class="id" class:pronoun>
    <span class="prefix {type}">{id.prefix}</span>{suffix}
</span>

<style>
    .id {
        pointer-events: all;
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

    .pronoun {
        color: inherit;
        pointer-events: none;
    }

    .pronoun > .prefix {
        color: inherit;
        font-weight: bold;
    }
</style>
