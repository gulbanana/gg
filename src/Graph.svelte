<script lang="ts">
    import { event } from "./ipc.js";
    import type { RepoStatus } from "./messages/RepoStatus.js";
    import type { LogNode } from "./messages/LogNode";
    import type { RefName } from "./messages/RefName";
    import type { RevId } from "./messages/RevId";
    import type { RevHeader } from "./messages/RevHeader.js";
    import GraphLine from "./GraphLine.svelte";
    import IdSpan from "./IdSpan.svelte";

    export let nodes: LogNode[];

    const repo_status = event<RepoStatus>("gg://repo/status");
    const change_content = event<RevHeader>("gg://change/select");

    interface Coordinates {
        row: number;
        column: number;
    }

    interface GraphRow extends Coordinates {
        skip_columns: number;
        id: RevId;
        desc: string;
        refs: RefName[];
        has_conflict: boolean;
        is_working_copy: boolean;
        select: () => void;
    }

    interface GraphLine {
        source: Coordinates;
        target: Coordinates;
        is_merge: boolean;
        has_elisions: boolean;
    }

    interface GraphStem {
        source: Coordinates;
        target: string;
        has_elisions: boolean;
    }

    // resolved primitives to draw
    let rows = new Array<GraphRow>();
    let lines = new Array<GraphLine>();

    // ongoing vertical lines
    let stems: (GraphStem | null)[] = [];

    // assign nodes to columnar stems
    for (let row = 0; row < nodes.length; row++) {
        let node = nodes[row];

        // find an existing stem targeting the current node
        let column = stems.length;
        let skip_columns = 0;
        for (let ixStem = 0; ixStem < stems.length; ixStem++) {
            let stem = stems[ixStem];
            if (stem != null && stem.target == node.revision.commit_id.prefix) {
                column = ixStem;
                skip_columns = stems.length - ixStem - 1;
                break;
            }
        }

        // terminate any existing stem, removing it from the end or leaving a gap
        // if there was no such stem, slot into any gaps that might exist
        if (column != -1) {
            let terminated_stem = stems[column];
            if (terminated_stem != null) {
                lines.push({
                    source: terminated_stem.source,
                    target: { row, column },
                    is_merge: false,
                    has_elisions: terminated_stem.has_elisions,
                });
            }
            stems[column] = null;
        } else {
            for (let ixStem = 0; ixStem < stems.length; ixStem++) {
                let stem = stems[ixStem];
                if (stem == null) {
                    column = ixStem;
                    skip_columns = stems.length - ixStem - 1;
                    break;
                }
            }
        }

        // remove empty stems on the right edge
        while (stems[stems.length - 1] === null) {
            stems.splice(stems.length - 1, 1);
        }

        // merge edges into existing stems or add new ones to the right
        for (let edge of node.edges) {
            if (edge.type != "Missing") {
                let target = edge.prefix;
                let ixExistingStem = stems.findIndex(
                    (s) => s?.target == target,
                );
                if (ixExistingStem != -1) {
                    lines.push({
                        source: { row, column },
                        target: { row: row + 1, column: ixExistingStem },
                        is_merge: true,
                        has_elisions: edge.type == "Indirect",
                    });
                } else {
                    let inserted_stem: GraphStem | null = {
                        source: { row, column },
                        target,
                        has_elisions: edge.type == "Indirect",
                    };
                    for (let ixStem = 0; ixStem < stems.length; ixStem++) {
                        if (stems[ixStem] === null) {
                            stems[ixStem] = inserted_stem;
                            inserted_stem = null;
                            break;
                        }
                    }
                    if (inserted_stem) {
                        stems.push(inserted_stem);
                    }
                }
            }
        }

        // create row
        rows.push({
            row,
            column,
            skip_columns,
            id: node.revision.change_id,
            desc: node.revision.description.lines[0],
            refs: node.revision.branches.filter(
                (r) =>
                    !(
                        r.remote != null &&
                        r.is_synced &&
                        node.revision.branches.find(
                            (r2) => r2.remote == null && r2.name == r.name,
                        )
                    ),
            ),
            is_working_copy:
                $repo_status?.working_copy?.prefix ==
                node.revision.commit_id.prefix,
            has_conflict: node.revision.has_conflict,
            select: () => ($change_content = node.revision),
        });
    }
</script>

<svg class="graph" style="width: 100%; height: {rows.length * 30}px;">
    {#each rows as node}
        <!-- svelte-ignore a11y-click-events-have-key-events -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <g
            class="row"
            transform="translate({node.column * 18} {node.row * 30})"
            on:click={node.select}
        >
            <rect
                class="row-backdrop"
                class:selected={$change_content?.change_id.prefix ==
                    node.id.prefix}
                rx="3"
                height="30"
                width="100%"
            />

            <circle cx="9" cy="15" r="6" fill="none" />
            {#if node.is_working_copy}
                <circle cx="9" cy="15" r="3" />
            {/if}

            <foreignObject
                class="row-html"
                x={node.skip_columns * 18 + 18}
                y="0"
                height="30"
                style="width: calc(100% - {(node.column + node.skip_columns) *
                    18 +
                    3}px)"
            >
                <div class="row-text" class:conflict={node.has_conflict}>
                    <code>
                        <IdSpan type="change" id={node.id} />
                    </code>
                    <span class="row-desc">
                        {node.desc}
                    </span>
                    {#each node.refs as ref}
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
    {#each lines as line}
        <GraphLine
            hasElisions={line.has_elisions}
            isMerge={line.is_merge}
            c1={line.source.column}
            c2={line.target.column}
            r1={line.source.row}
            r2={line.target.row}
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
