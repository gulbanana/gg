<script lang="ts">
    import type { ChangeId } from "../messages/ChangeId";
    import type { CommitId } from "../messages/CommitId";
    import { currentTarget } from "../stores";
    export let id: ChangeId | CommitId;
    export let pronoun: boolean = false;

    let suffix = id.rest.substring(0, 8 - id.prefix.length);
</script>

<span class="id" class:pronoun={pronoun || $currentTarget?.type == "Repository"}>
    <span class="prefix {id.type}">{id.prefix}</span>{suffix}
</span>

<style>
    .id {
        pointer-events: all;
        user-select: text;
        color: var(--ctp-subtext1);
        font-family: var(--stack-code);
    }

    .ChangeId {
        color: var(--ctp-pink);
    }

    .CommitId {
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
