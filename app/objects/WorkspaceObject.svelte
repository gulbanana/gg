<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import Chip from "../controls/Chip.svelte";
    import Icon from "../controls/Icon.svelte";
    import Object from "./Object.svelte";

    export let name: string;

    $: operand = { type: "Workspace", name } as Operand;
</script>

<Object {operand} label={name} conflicted={false} let:context>
    <Chip {context} target={false} immobile tip="workspace: {name}">
        <Icon name="at-sign" state="change" />
        <span>{name}</span>
    </Chip>
</Object>
