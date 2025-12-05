<script lang="ts">
    import type { RevHeader } from "../messages/RevHeader";
    import type { StoreRef } from "../messages/StoreRef";
    import type { Operand } from "../messages/Operand";
    import Chip from "../controls/Chip.svelte";

    let { header, ref }: {
        header: RevHeader;
        ref: Extract<StoreRef, { type: "Tag" }>;
    } = $props();

    let operand = $derived<Operand>({ type: "Ref", header, ref });
</script>

<Chip context={false} target={false} immobile tip="tag">
    <span>{ref.tag_name}</span>
</Chip>
