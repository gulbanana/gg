<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { StoreRef } from "../messages/StoreRef";
    import type { Operand } from "../messages/Operand";
    import Icon from "../controls/Icon.svelte";
    import Chip from "../controls/Chip.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    export let header: RevHeader;
    export let ref: Extract<StoreRef, { type: "LocalBranch" | "RemoteBranch" }>;

    let label: string;
    let state: "add" | "change" | "remove";
    let disconnected: boolean;
    switch (ref.type) {
        case "LocalBranch":
            label = ref.branch_name;
            state = ref.is_synced ? "change" : "add";
            disconnected = !ref.is_tracking;
            break;
        case "RemoteBranch":
            label = `${ref.branch_name}@${ref.remote_name}`;
            state = ref.is_tracked ? "remove" : "change";
            disconnected = ref.is_deleted;
            break;
    }

    let operand: Operand = { type: "Ref", header, ref };
</script>

<Object {operand} {label} conflicted={ref.has_conflict} let:context let:hint>
    <Zone {operand} let:target>
        <Chip {context} {target} {disconnected}>
            <Icon name="git-branch" state={context ? null : state} />
            <span>{hint ?? label}</span>
        </Chip>
    </Zone>
</Object>
