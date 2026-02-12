<script lang="ts">
    import type { RevChange } from "../messages/RevChange";
    import type { RevHeader } from "../messages/RevHeader";
    import type { Operand } from "../messages/Operand";
    import type { ExternalDiff } from "../messages/ExternalDiff";
    import Icon from "../controls/Icon.svelte";
    import ActionWidget from "../controls/ActionWidget.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";
    import { changeSelectEvent, repoConfigEvent } from "../stores";
    import { mutate } from "../ipc";

    export let headers: RevHeader[] | null;
    export let change: RevChange;
    export let selected: boolean;

    let operand: Operand | null = headers ? { type: "Change", headers, path: change.path, hunk: null } : null;

    $: hasDiffTool = $repoConfigEvent.type === "Workspace" && $repoConfigEvent.has_external_diff_tool;

    let icon = "file";
    let state: "add" | "change" | "remove" | null = null;
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

    function onSelect() {
        changeSelectEvent.set(change);
    }

    function onExternalDiff() {
        if (!headers) return;
        mutate<ExternalDiff>("external_diff", {
            id: headers[0].id,
            path: change.path,
        });
    }
</script>

<Object
    {operand}
    {selected}
    suffix={change.path.repo_path}
    conflicted={change.has_conflict}
    label={change.path.relative_path}
    on:click={onSelect}
    let:context
    let:hint>
    <Zone {operand} let:target>
        <div class="layout" class:target>
            <Icon name={icon} state={context ? null : state} />
            <span>{hint ?? change.path.relative_path}</span>
            {#if hasDiffTool && operand}
                <ActionWidget safe tip="open in diff tool" onClick={onExternalDiff}>
                    <Icon name="external-link" />
                </ActionWidget>
            {/if}
        </div>
    </Zone>
</Object>

<style>
    .layout {
        height: 30px;
        display: flex;
        align-items: center;
        gap: 6px;
        padding-left: 3px;
    }

    .layout span {
        flex: 1;
    }

    .layout.target {
        background: var(--ctp-flamingo);
        color: black;
    }
</style>
