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

    let rows = formatNodes(nodes);
    
    interface GraphNode {
        row: number;
        column: number;
        skip_columns: number;

        id: RevId;
        parents: GraphEdge[];

        desc: string;
        refs: RefName[],
        has_conflict: boolean;
        is_working_copy: boolean;
        select: () => void;
    }

    interface GraphEdge {
        row: number;
        column: number;

        id: RevId;

        has_elisions: boolean;
    }

    function formatNodes(nodes: LogNode[]): GraphNode[] {
        let graph: GraphNode[] = [];

        let row = 0;
        for (let n of nodes) {
            let children = graph.filter(n2 => n2.parents.find(p => p.id.prefix == n.revision.commit_id.prefix));

            let column = 0;
            if (row != 0) {
                if (children.length == 0) {
                    column = graph[row-1].column + 1;
                } else {
                    let leftmost_child = children.sort((a, b) => a.column - b.column)[0];
                    let child_index = 0;
                    let skip_index = 0;
                    for (let p of leftmost_child.parents) {
                        if (nodes.findIndex(g => g.revision.commit_id.prefix == p.id.prefix) > row) {
                            child_index++;
                        } else {
                            skip_index++;
                        }
                    }
                    column = leftmost_child.column + child_index;
                }
            }       
            
            for (let child of children) {                
                let parent = child.parents.find(p => p.id.prefix == n.revision.commit_id.prefix);
                if (parent) {
                    parent.row = row;
                    parent.column = column;
                }
            }

            let node = {
                row: row++,
                column,
                skip_columns: 0,
                id: n.revision.change_id,
                parents: n.edges.flatMap<GraphEdge>(e => {
                    switch (e.type) {
                        case "Direct":
                            return [{ row: -1, column: -1, id: e, has_elisions: false}];
                        case "Indirect":
                            return [{ row: -1, column: -1, id: e, has_elisions: true}];
                        case "Missing":
                            return [];
                    }
                }),
                desc: n.revision.description.lines[0],
                refs: n.revision.branches.filter(r => !(r.remote != null && r.is_synced && n.revision.branches.find(r2 => r2.remote == null && r2.name == r.name))),
                is_working_copy: $repo_status?.working_copy?.prefix == n.revision.commit_id.prefix,
                has_conflict: n.revision.has_conflict,
                select: () => ($change_content = n.revision),
            };

            graph.push(node);
        }

        let active_branches = new Map<number, number>();
        for (let n of graph) {
            active_branches.delete(n.row);
            let max_branch = Math.max(0, ...active_branches.values());
            if (max_branch > n.column) {
                n.skip_columns = max_branch - n.column;
                console.log(`${n.id.prefix} skip ${n.skip_columns} due to ${active_branches.size} branches`);
                console.log(Array.from(active_branches.values()));
            }            
            for (let p of n.parents) {
                active_branches.set(p.row, Math.max(active_branches.get(p.row) || 0, n.column));
            }
        }

        return graph;
    }
</script>

<svg
class="graph"
style="width: 100%; height: {rows.length * 30}px;"
>
    {#each rows as node}
        <!-- svelte-ignore a11y-click-events-have-key-events -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <g
            class="row"
            transform="translate({node.column * 18} {node.row *
                30})"
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

            <foreignObject class="row-html" x={node.skip_columns * 18 + 18} y="0" height="30">
                <div class="row-text" class:conflict={node.has_conflict}>
                    <code>
                        <IdSpan type="change" id={node.id} />
                    </code>
                    <span class="row-desc">
                        {node.desc}
                    </span>
                    {#each node.refs as ref}
                        <code class="tag" class:conflict={ref.has_conflict}>
                            {ref.remote == null ? ref.name : `${ref.name}@${ref.remote}`}
                        </code>
                    {/each}
                </div>
            </foreignObject>
        </g>
    {/each}
    {#each rows as node}
        {#each node.parents as parent}
            <GraphLine has_elisions={parent.has_elisions} c1={node.column} c2={parent.column} r1={node.row} r2={parent.row} />
        {/each}
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
        width: calc(100% - 18px);
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