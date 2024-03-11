<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { RefName } from "../messages/RefName";
    import type { Operand } from "../messages/Operand";
    import Icon from "../controls/Icon.svelte";
    import Chip from "../controls/Chip.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    export let header: RevHeader;
    export let name: RefName;

    let label: string;
    let state: "add" | "change" | "remove";
    switch (name.type) {
        case "LocalBranch":
            label = name.branch_name;
            state = name.is_synced ? "change" : "add";
            break;
        case "RemoteBranch":
            label = `${name.branch_name}@${name.remote_name}`;
            state = name.is_tracked ? "remove" : "change";
            break;
    }

    let operand: Operand = { type: "Branch", header, name };
</script>

<Object {operand} {label} conflicted={name.has_conflict} let:context>
    <Zone {operand} let:target>
        <Chip {context} {target}>
            <Icon name="git-branch" state={context ? null : state} />
            <span>{label}</span>
        </Chip>
    </Zone>
</Object>
