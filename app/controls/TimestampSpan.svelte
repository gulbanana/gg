<script lang="ts">
    import { lastFocus } from "../stores";
    export let timestamp: string;

    let datetime = new Date(timestamp);

    function relativeDate(now: number) {
        const cutoffs = [60, 3600, 86400, 86400 * 7, 86400 * 30, 86400 * 365, Infinity];
        const units: Intl.RelativeTimeFormatUnit[] = ["second", "minute", "hour", "day", "week", "month", "year"];
        const formatter = new Intl.RelativeTimeFormat(undefined, {
            style: "long",
            numeric: "auto",
        });
        let ticks = datetime.getTime();
        let seconds = Math.round((ticks - now) / 1000);
        let unitIndex = cutoffs.findIndex((cutoff) => cutoff > Math.abs(seconds));
        let divisor = unitIndex ? cutoffs[unitIndex - 1] : 1;

        return formatter.format(Math.floor(seconds / divisor), units[unitIndex]);
    }
</script>

<span class="timestamp" title={datetime.toLocaleString()}>
    {relativeDate($lastFocus)}
</span>

<style>
    .timestamp {
        color: var(--ctp-subtext0);
        white-space: nowrap;
        pointer-events: auto;
    }
</style>
