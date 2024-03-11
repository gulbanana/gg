<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader";
    import type { RefName } from "./messages/RefName";
    import type { MenuContext } from "./messages/MenuContext";
    import { currentContext } from "./stores.js";
    import Icon from "./Icon.svelte";
    import { command } from "./ipc";

    export let rev: RevHeader;
    export let ref: RefName;

    let state: "add" | "change" | "remove";
    switch (ref.type) {
        case "LocalBranch":
            state = ref.is_synced ? "change" : "add";
            break;
        case "RemoteBranch":
            state = ref.is_tracked ? "remove" : "change";
            break;
    }

    let is_context = false;
    $: is_context =
        $currentContext?.type == "Branch" && ref == $currentContext?.name;

    function onMenu(event: Event) {
        event.preventDefault();
        event.stopPropagation();

        let context: MenuContext = { type: "Branch", rev, name: ref };
        currentContext.set(context);

        command("forward_context_menu", { context });
    }
</script>

<button
    class="unbutton chip"
    class:conflict={ref.has_conflict}
    class:context={is_context}
    on:contextmenu={onMenu}>
    <Icon name="git-branch" state={is_context ? null : state} />
    <span>
        {#if ref.type == "LocalBranch"}
            {ref.branch_name}
        {:else}
            {ref.branch_name}@{ref.remote_name}
        {/if}
    </span>
</button>

<style>
    .chip {
        font-family: var(--stack-code);
        font-size: smaller;
        color: var(--ctp-text);

        height: 24px;
        line-height: 16px;

        display: flex;
        align-items: center;
        border: 1px solid var(--ctp-overlay1);
        border-radius: 12px;
        padding: 0 6px;
        background: var(--ctp-crust);
        white-space: nowrap;
        gap: 3px;

        cursor: pointer;
    }

    .context {
        border-color: var(--ctp-rosewater);
        color: var(--ctp-rosewater);
    }
</style>
