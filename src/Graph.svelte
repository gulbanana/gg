<script lang="ts">
    import { event } from "./ipc.js";
    import type { RepoStatus } from "./messages/RepoStatus.js";
    import type { LogPage } from "./messages/LogPage";
    import type { RevHeader } from "./messages/RevHeader.js";
    import GraphLine from "./GraphLine.svelte";
    import IdSpan from "./IdSpan.svelte";

    export let page: LogPage;

    const repo_status = event<RepoStatus>("gg://repo/status");
    const change_content = event<RevHeader>("gg://change/select");
</script>

<svg class="graph" style="width: 100%; height: {page.rows.length * 30}px;">
    {#each page.rows as row}
        <!-- svelte-ignore a11y-click-events-have-key-events -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <g
            class="row"
            transform="translate({row.location[0] * 18} {row.location[1] * 30})"
            on:click={() => ($change_content = row.revision)}
        >
            <rect
                class="row-backdrop"
                class:selected={$change_content?.change_id.prefix ==
                    row.revision.change_id.prefix}
                rx="3"
                height="30"
                width="100%"
            />

            <circle cx="9" cy="15" r="6" fill="none" />
            {#if $repo_status?.working_copy?.prefix == row.revision.commit_id.prefix}
                <circle cx="9" cy="15" r="3" />
            {/if}

            <foreignObject
                class="row-html"
                x={row.padding * 18 + 18}
                y="0"
                height="30"
                style="width: calc(100% - {(row.location[0] + row.padding) *
                    18 +
                    3}px)"
            >
                <div
                    class="row-text"
                    class:conflict={row.revision.has_conflict}
                >
                    <code>
                        <IdSpan type="change" id={row.revision.change_id} />
                    </code>
                    <span class="row-desc">
                        {row.revision.description.lines[0]}
                    </span>
                    {#each row.revision.branches as ref}
                        <code class="tag" class:conflict={ref.has_conflict}>
                            {ref.remote == null
                                ? ref.name
                                : `${ref.name}@${ref.remote}`}
                        </code>
                    {/each}
                </div>
            </foreignObject>
        </g>
    {/each}

    {#each page.lines as line}
        <GraphLine
            hasElisions={line.indirect}
            isMerge={line.type == "ToIntersection"}
            allowEarlyBreak={line.type == "FromNode"}
            c1={line.source[0]}
            c2={line.target[0]}
            r1={line.source[1]}
            r2={line.target[1]}
        />
    {/each}
</svg>

<style>
    svg {
        stroke: var(--ctp-text);
        fill: var(--ctp-text);
    }

    .row {
        cursor: pointer;
    }

    .row-backdrop {
        stroke: none;
        fill: transparent;
    }

    .row-backdrop:global(.selected) {
        fill: var(--ctp-base);
    }

    .row-html {
        overflow: hidden;
    }

    .row-text {
        height: 100%;
        display: flex;
        align-items: center;
        gap: 6px;
        margin-right: 15px;
    }

    .row-desc {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        flex: 1;
    }

    .tag {
        height: 24px;
        display: flex;
        align-items: center;
        border: 1px solid var(--ctp-overlay1);
        border-radius: 12px;
        padding: 0 6px;
        background: var(--ctp-crust);
        white-space: nowrap;
    }

    /* both nodes and refs can have this */
    .conflict {
        background: repeating-linear-gradient(
            120deg,
            transparent 0px,
            transparent 12px,
            var(--ctp-surface0) 12px,
            var(--ctp-surface0) 15px
        );
    }
</style>
