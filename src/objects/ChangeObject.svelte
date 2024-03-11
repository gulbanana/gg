<script lang="ts">
    import type { RevChange } from "../messages/RevChange";
    import type { RevHeader } from "../messages/RevHeader";
    import type { Operand } from "../messages/Operand";
    import Icon from "../controls/Icon.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    export let header: RevHeader;
    export let change: RevChange;

    let operand: Operand = { type: "Change", header, path: change.path };

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
</script>

<Object {operand} conflicted={change.has_conflict} label={change.path.relative_path} let:context>
    <Zone {operand} let:target>
        <div class="layout" class:target>
            <Icon name={icon} state={context ? null : state} />
            <span>{change.path.relative_path}</span>
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
