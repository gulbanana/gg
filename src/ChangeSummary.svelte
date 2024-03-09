<script lang="ts">
    import Icon from "./Icon.svelte";
    import type { RevChange } from "./messages/RevChange";

    export let change: RevChange;

    let icon: string;
    let className: string;
    switch (change.kind) {
        case "Added":
            icon = "file-plus";
            className = "added";
            break;
        case "Deleted":
            icon = "file-minus";
            className = "deleted";
            break;
        case "Modified":
            icon = "file";
            className = "modified";
            break;
    }
</script>

<button
    class="unbutton layout {className}"
    class:conflict={change.has_conflict}>
    <Icon name={icon} />
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

    .added {
        color: var(--ctp-green);
    }

    .modified {
        color: var(--ctp-blue);
    }

    .deleted {
        color: var(--ctp-red);
    }
</style>
