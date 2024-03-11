<script lang="ts">
    import Icon from "./Icon.svelte";
    import type { MenuContext } from "./messages/MenuContext";
    import type { RevChange } from "./messages/RevChange";
    import type { RevHeader } from "./messages/RevHeader";
    import { currentContext } from "./stores.js";
    import { command } from "./ipc";

    export let rev: RevHeader;
    export let change: RevChange;

    let icon: string;
    let state: "add" | "change" | "remove";
    switch (change.kind) {
        case "Added":
            icon = "file-plus";
            state = "add";
            break;
        case "Deleted":
            icon = "file-minus";
            state = "remove";
            break;
        case "Modified":
            icon = "file";
            state = "change";
            break;
    }

    let is_context = false;
    $: is_context =
        $currentContext?.type == "Tree" && change.path == $currentContext.path;

    function onMenu(event: Event) {
        event.preventDefault();
        event.stopPropagation();

        let context: MenuContext = { type: "Tree", rev, path: change.path };
        currentContext.set(context);

        command("forward_context_menu", { context });
    }
</script>

<button
    class="unbutton layout"
    class:conflict={change.has_conflict}
    class:context={is_context}
    tabindex="-1"
    on:contextmenu={onMenu}>
    <Icon name={icon} state={is_context ? null : state} />
    <span>{change.path.relative_path}</span>
</button>

<style>
    .layout {
        display: flex;
        align-items: center;
        cursor: pointer;
        gap: 6px;
        padding-left: 3px;
    }

    .layout.conflict {
        background: repeating-linear-gradient(
            120deg,
            transparent 0px,
            transparent 12px,
            var(--ctp-surface0) 12px,
            var(--ctp-surface0) 15px
        );
    }

    .context {
        color: var(--ctp-rosewater);
    }
</style>
