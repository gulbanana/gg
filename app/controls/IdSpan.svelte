<script lang="ts">
    import type { ChangeId } from "../messages/ChangeId";
    import type { CommitId } from "../messages/CommitId";
    import { currentTarget } from "../stores";
    export let id: ChangeId | CommitId;
    export let pronoun: boolean = false;
    export let selectable: boolean = false;

    $: suffix = id.rest.substring(0, 8 - id.prefix.length);
    $: category = id.type == "CommitId" ? "commit" : id.is_divergent ? "divergent" : id.offset ? "hidden" : "change";
</script>

<span class="id" class:pronoun={pronoun || $currentTarget?.type == "Repository"} class:selectable>
    <span class="prefix {category}">{id.prefix}</span>{suffix}{#if id.type == "ChangeId" && id.offset}<span
            class="suffix {category}">/{id.offset}</span
        >{/if}
</span>

<style>
    .id {
        pointer-events: none;
        color: var(--ctp-subtext0);
        font-family: var(--stack-code);
    }

    .commit {
        color: var(--ctp-mauve);
    }

    .change {
        color: var(--ctp-pink);
    }

    .hidden {
        color: var(--ctp-text);
    }

    .divergent {
        color: var(--ctp-red);
    }

    .pronoun {
        color: inherit;
        pointer-events: none;
    }

    .pronoun > .prefix,
    .pronoun > .suffix {
        color: inherit;
        font-weight: bold;
    }

    .selectable {
        user-select: text;
    }
</style>
