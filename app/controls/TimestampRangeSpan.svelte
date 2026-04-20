<script lang="ts">
    export let from: string;
    export let to: string;

    $: fromDatetime = new Date(from);
    $: toDatetime = new Date(to);

    const formatter = new Intl.DateTimeFormat(undefined, {
        dateStyle: "short",
        timeStyle: "short",
    });
</script>

<span class="timestamp" title={`${fromDatetime.toLocaleString()} - ${toDatetime.toLocaleString()}`}>
    {formatter.formatRange(fromDatetime, toDatetime)}
</span>

<style>
    .timestamp {
        color: var(--gg-colors-foregroundMuted);
        white-space: nowrap;
        pointer-events: auto;
    }
</style>
