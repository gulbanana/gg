<script lang="ts">
    import type { RevChange } from "../messages/RevChange";
    import type { RevHeader } from "../messages/RevHeader";
    import type { Operand } from "../messages/Operand";
    import Icon from "../controls/Icon.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";
    import { changeSelectEvent } from "../stores";

    export let header: RevHeader;
    export let change: RevChange;
    export let selected: boolean;

    let operand: Operand = { type: "Change", header, path: change.path };

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
        </div>
    </Zone>
</Object>

<style>
    .layout {
        height: 30px;
        display: flex;
        align-items: center;
        cursor: pointer;
        gap: 6px;
        padding-left: 3px;
    }

    .layout.target {
        background: var(--ctp-flamingo);
        color: black;
    }
</style>
