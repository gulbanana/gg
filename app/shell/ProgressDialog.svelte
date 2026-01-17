<script lang="ts">
    import ProgressWidget from "../controls/ProgressWidget.svelte";
    import type { ProgressEvent } from "../messages/ProgressEvent";

    export let progress: ProgressEvent | undefined;

    let label = "Working...";
    let value: number | undefined = undefined;

    $: if (progress) {
        if (progress.type === "Progress") {
            value = progress.overall_percent;
        } else if (progress.type === "Message") {
            label = progress.text;
        }
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
