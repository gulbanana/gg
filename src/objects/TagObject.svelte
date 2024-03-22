<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { StoreRef } from "../messages/StoreRef";
    import type { Operand } from "../messages/Operand";
    import Chip from "../controls/Chip.svelte";

    export let header: RevHeader;
    export let ref: Extract<StoreRef, { type: "Tag" }>;

    let operand: Operand = { type: "Ref", header, ref };
</script>

<Chip context={false} target={false} immobile tip="tag">
    <span>{ref.tag_name}</span>
</Chip>
