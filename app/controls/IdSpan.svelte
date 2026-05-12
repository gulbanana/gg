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
        color: var(--gg-colors-foregroundMuted);
        font-family: var(--gg-text-familyCode);
    }

    .commit {
        color: var(--gg-colors-accent);
    }

    .change {
        color: var(--gg-colors-accent);
    }

    .hidden {
        color: var(--gg-colors-foreground);
    }

    .divergent {
        color: var(--gg-colors-error);
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
