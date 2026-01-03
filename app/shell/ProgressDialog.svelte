<script lang="ts">
    import ProgressWidget from "../controls/ProgressWidget.svelte";
    import type { ProgressEvent } from "../messages/ProgressEvent";

    export let progress: ProgressEvent | undefined;

    let label = "Working...";
    let value: number | undefined = undefined;

    $: if (progress) {
        if (progress.type === "Progress") {
            value = progress.overall_percent;
            if (progress.bytes_downloaded != null) {
                label = `${formatBytes(Number(progress.bytes_downloaded))} downloaded`;
            }
        } else if (progress.type === "Message") {
            label = progress.text;
        }
    }

    function formatBytes(bytes: number): string {
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    }
</script>

<div id="progress-chrome">
    <ProgressWidget {label} {value} />
</div>

<style>
    #progress-chrome {
        grid-area: 2/2/2/2;
        min-width: 300px;
    }
</style>
