<script lang="ts">
    import type { RevChange } from "../messages/RevChange";
    import type { RevHeader } from "../messages/RevHeader";
    import type { Operand } from "../messages/Operand";
    import type { ExternalDiff } from "../messages/ExternalDiff";
    import type { ExternalResolve } from "../messages/ExternalResolve";
    import Icon from "../controls/Icon.svelte";
    import ActionWidget from "../controls/ActionWidget.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";
    import { changeSelectEvent, repoConfigEvent } from "../stores";
    import { mutate } from "../ipc";
    import ActionLink from "../controls/ActionLink.svelte";

    export let headers: RevHeader[] | null;
    export let change: RevChange;
    export let selected: boolean;

    let operand: Operand | null = headers ? { type: "Change", headers, path: change.path, hunk: null } : null;

    $: hasDiffTool = $repoConfigEvent.type === "Workspace" && $repoConfigEvent.has_external_diff_tool;
    $: hasMergeTool = $repoConfigEvent.type === "Workspace" && $repoConfigEvent.has_external_merge_tool;

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

    function splitPath(p: string): [string, string] {
        let sep = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
        return sep >= 0 ? [p.slice(0, sep), p.slice(sep)] : ["", p];
    }

    function onExternalResolve() {
        if (!headers) return;
        mutate<ExternalResolve>("external_resolve", {
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
            <span class="chevron" class:expanded={selected}>
                <Icon name="chevron-right" />
            </span>
            <Icon name={icon} state={context ? null : state} />
            <span class="path-prefix">{splitPath(hint ?? change.path.relative_path)[0]}</span><span class="path-suffix">{splitPath(hint ?? change.path.relative_path)[1]}</span>
            {#if hasMergeTool && change.has_conflict && operand}
                <ActionWidget tip="resolve in merge tool" onClick={onExternalResolve}>
                    <Icon name="external-link" /> Resolve
                </ActionWidget>
            {:else if hasDiffTool && operand}
                <ActionLink tip="open in diff tool" onClick={onExternalDiff}>
                    <Icon name="external-link" />
                </ActionLink>
            {/if}
        </div>
    </Zone>
</Object>

<style>
    .layout {
        height: 30px;
        display: flex;
        align-items: center;
        gap: 3px;
        padding-left: 3px;
        border-bottom: 1px solid var(--gg-colors-surface);
    }

    .path-prefix {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        flex-shrink: 1;
        min-width: 0;
    }

    .path-suffix {
        white-space: nowrap;
        flex-shrink: 0;
    }

    .chevron {
        display: flex;
        align-items: center;
        flex: none;
        transition: transform 150ms ease;
        color: var(--gg-colors-foregroundSubtle);
    }

    :global(.selected) .chevron {
        color: var(--gg-colors-selectionForeground);
    }

    .chevron.expanded {
        transform: rotate(90deg);
    }

    .layout.target {
        background: var(--gg-colors-accent);
        color: var(--gg-colors-foreground);
    }
</style>
