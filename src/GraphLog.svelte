<!-- Renders commit rows with an SVG graph drawn over them -->

<script lang="ts">
  import type { LogRow } from "./messages/LogRow.js";
  import GraphLine from "./GraphLine.svelte";
  import { repoStatus } from "./events.js";

  export let rows: LogRow[];
  interface $$Slots {
    default: { row: LogRow };
  }
</script>

<svg class="graph" style="width: 100%; height: {rows.length * 30}px;">
  {#each rows as row}
    <g transform="translate({row.location[0] * 18} {row.location[1] * 30})">
      <foreignObject
        class="row"
        height="30"
        style="width: calc(100% - {(row.location[0] + row.padding) * 18 +
          18}px); --leftpad: {row.padding * 18 + 18}px;"
      >
        <slot {row} />
      </foreignObject>

      <circle cx="9" cy="15" r="6" fill="none" />
      {#if $repoStatus?.working_copy?.prefix == row.revision.commit_id.prefix}
        <circle cx="9" cy="15" r="3" />
      {/if}
    </g>

    {#each row.lines as line}
      <GraphLine {line} />
    {/each}
  {/each}
</svg>

<style>
  svg {
    stroke: var(--ctp-text);
    fill: var(--ctp-text);
  }

  circle {
    pointer-events: none;
  }

  .row {
    overflow: hidden;
  }
</style>
