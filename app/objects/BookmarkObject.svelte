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

    let settings = getContext<Settings>("settings");

    $: operand = { type: "Ref", header, ref } as Operand;

    $: label = ref.type === "LocalBookmark" ? ref.bookmark_name : `${ref.bookmark_name}@${ref.remote_name}`;

    $: state = (
        ref.type === "LocalBookmark" ? (ref.is_synced ? "change" : "add") : ref.is_tracked ? "remove" : "change"
    ) as "add" | "change" | "remove";

    $: disconnected =
        settings.markUnpushedBookmarks &&
        (ref.type === "LocalBookmark"
            ? ref.available_remotes == 0 && ref.potential_remotes > 0
            : ref.is_tracked && ref.is_absent);

    $: tip = computeTip(ref);

    function computeTip(ref: Extract<StoreRef, { type: "LocalBookmark" | "RemoteBookmark" }>): string {
        if (ref.type === "LocalBookmark") {
            if (ref.available_remotes == 0 && ref.potential_remotes > 0) {
                return "local-only bookmark";
            }
            let result = "local bookmark";
            if (ref.tracking_remotes.length >= 0) {
                result = result + " (tracking ";
                let first = true;
                for (let r of ref.tracking_remotes) {
                    if (first) {
                        first = false;
                    } else {
                        result = result + ", ";
                    }
                    result = result + r;
                }
                result = result + ")";
            }
            return result;
        } else {
            let result = "remote bookmark";
            if (ref.is_tracked && ref.is_absent) {
                return result + " (deleting)";
            } else if (ref.is_tracked) {
                return result + " (tracked)";
            } else {
                return result + " (untracked)";
            }
        }
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
