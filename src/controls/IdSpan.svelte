<script lang="ts">
    import type { ChangeId } from "../messages/ChangeId";
    import type { CommitId } from "../messages/CommitId";
    import { currentTarget } from "../stores";
    export let id: ChangeId | CommitId;
    export let pronoun: boolean = false;
    export let clickable: boolean = false;

    let suffix = id.rest.substring(0, 8 - id.prefix.length);

    async function copyId() {
        const shortestId = id.prefix + suffix.substring(0, 2);
        try {
            await navigator.clipboard.writeText(shortestId);
        } catch (err) {
            console.error("Failed to copy to clipboard:", err);
        }
    }

    $: isClickable = clickable && !pronoun && $currentTarget?.type != "Repository";
</script>

<button
    class="id"
    class:pronoun={pronoun || $currentTarget?.type == "Repository"}
    class:clickable={isClickable}
    disabled={!isClickable}
    on:click={copyId}
    title={isClickable ? "Click to copy ID" : ""}>
    <span class="prefix {id.type}">{id.prefix}</span>{suffix}
</button>

<style>
    .id {
        color: var(--ctp-subtext1);
        font-family: var(--stack-code);

        /* reset button style, make it look like text */
        background: none;
        border: none;
        padding: 0;
        margin: 0;
        font: inherit;
        cursor: inherit;
    }

    .id:disabled {
        pointer-events: none;
    }

    .id.clickable {
        pointer-events: auto;
        cursor: pointer;
    }

    .id.clickable:hover {
        background-color: var(--ctp-surface1);
    }

    .id.clickable:active {
        background-color: var(--ctp-surface2);
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
