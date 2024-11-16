<script lang="ts">
    import { getContext } from "svelte";
    import type { RevHeader } from "../messages/RevHeader";
    import type { StoreRef } from "../messages/StoreRef";
    import type { Operand } from "../messages/Operand";
    import type Settings from "../shell/Settings";
    import Icon from "../controls/Icon.svelte";
    import Chip from "../controls/Chip.svelte";
    import Object from "./Object.svelte";
    import Zone from "./Zone.svelte";

    export let header: RevHeader;
    export let ref: Extract<StoreRef, { type: "LocalBookmark" | "RemoteBookmark" }>;

    let operand: Operand = { type: "Ref", header, ref };

    let label: string;
    let state: "add" | "change" | "remove";
    let disconnected: boolean;
    let tip: string;

    switch (ref.type) {
        case "LocalBookmark":
            label = ref.branch_name;
            state = ref.is_synced ? "change" : "add";
            disconnected = ref.available_remotes == 0 && ref.potential_remotes > 0;

            if (disconnected) {
                tip = "local-only bookmark";
            } else {
                tip = "local bookmark";
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

        case "RemoteBookmark":
            label = `${ref.branch_name}@${ref.remote_name}`;
            state = ref.is_tracked ? "remove" : "change"; // we haven't combined this remote, and it has a local = red
            disconnected = ref.is_tracked && ref.is_absent;

            tip = "remote bookmark";
            if (disconnected) {
                tip = tip + " (deleting)";
            } else if (ref.is_tracked) {
                tip = tip + " (tracked)";
            } else {
                tip = tip + " (untracked)";
            }

            break;
    }

    if (!getContext<Settings>("settings").markUnpushedBranches) {
        disconnected = false;
    }
</script>

<Object {operand} {label} conflicted={ref.has_conflict} let:context let:hint={dragHint}>
    <Zone {operand} let:target let:hint={dropHint}>
        <Chip {context} {target} {disconnected} {tip}>
            <Icon name="bookmark" state={context ? null : state} />
            <span>{dragHint ?? dropHint ?? label}</span>
        </Chip>
    </Zone>
</Object>
