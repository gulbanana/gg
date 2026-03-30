<script lang="ts">
    import Chip from "../controls/Chip.svelte";
    import Icon from "../controls/Icon.svelte";

    export let name: string;
</script>

<Chip context={false} target={false} immobile tip="workspace: {name}">
    <Icon name="at-sign" state="change" />
    <span>{name}</span>
</Chip>
