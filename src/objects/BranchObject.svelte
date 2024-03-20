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
    let tip: string;

    switch (ref.type) {
        case "LocalBranch":
            label = ref.branch_name;
            state = ref.is_synced ? "change" : "add";
            disconnected = ref.available_remotes == 0 && ref.potential_remotes > 0;

            if (disconnected) {
                tip = "local-only branch";
            } else {
                tip = "local branch";
                if (ref.tracking_remotes.length >= 0) {
                    tip = tip + " (tracking ";
                    let first = true;
                    for (let r of ref.tracking_remotes) {
                        if (first) {
                            first = false;
                        } else {
                            tip = tip + ", ";
                        }
                        tip = tip + r;
                    }
                    tip = tip + ")";
                }
            }

            break;

        case "RemoteBranch":
            label = `${ref.branch_name}@${ref.remote_name}`;
            state = ref.is_tracked ? "remove" : "change"; // we haven't combined this remote, and it has a local = red
            disconnected = ref.is_tracked && ref.is_absent;

            tip = "remote branch";
            if (disconnected) {
                tip = tip + " (deleting)";
            } else if (ref.is_tracked) {
                tip = tip + " (tracked)";
            } else {
                tip = tip + " (untracked)";
            }

            break;
    }

    let operand: Operand = { type: "Ref", header, ref };
</script>

<Object {operand} {label} {tip} conflicted={ref.has_conflict} let:context let:hint>
    <Zone {operand} let:target>
        <Chip {context} {target} {disconnected}>
            <Icon name="git-branch" state={context ? null : state} />
            <span>{hint ?? label}</span>
        </Chip>
    </Zone>
</Object>
