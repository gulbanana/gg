<script lang="ts">
    import type { RevSet } from "../messages/RevSet";
    import { currentTarget } from "../stores";
    import IdSpan from "./IdSpan.svelte";

    export let set: RevSet;
    export let selectable: boolean = false;
</script>

<!-- prettier-ignore -->
<span class="set" class:pronoun={$currentTarget?.type == "Repository"} class:selectable><IdSpan id={set.from.change} {selectable} /><span class="separator">::</span><IdSpan id={set.to.change} {selectable} /></span>

<style>
    .set {
        pointer-events: none;
    }

    .separator {
        color: var(--ctp-subtext1);
        font-family: var(--stack-code);
    }

    .pronoun {
        color: inherit;
        pointer-events: none;
    }

    .pronoun > .separator {
        color: inherit;
    }

    .selectable {
        user-select: all;
    }
</style>
