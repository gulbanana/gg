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

    let rows = checkNodes(nodes);
    
    interface GraphNode {
        row: number;
        column: number;
        skip_columns: number;

        id: RevId;
        ancestors: GraphEdge[];

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

        hasElisions: boolean;
    }

    // assign nodes to columnar stems
    function checkNodes(nodes: LogNode[]): GraphNode[] {
        let graph: GraphNode[] = [];

        let stems: { next: string, prev: string }[] = [];

        for (let ixNode = 0; ixNode < nodes.length; ixNode++) {
            let node = nodes[ixNode];
            let key = node.revision.commit_id.prefix;

            console.log(`${node.revision.change_id.prefix}/${node.revision.commit_id.prefix} stems, before: `, stems.map(s => {return { head: s.next }}));

            // determine stems consumed by current node            
            let nodeStems: number[] = [];
            for (let ixStem = 0; ixStem < stems.length; ixStem++) {
                if (stems[ixStem].next == node.revision.commit_id.prefix) {
                    nodeStems.push(ixStem);
                }
            }            
            let nodeStem = nodeStems.length > 0 ? nodeStems[0] : stems.length;

            // each stem terminating in the current node should be removed *or*, in up to 1 case, replaced by an edge's target (see below)
            for (let terminatedStem of nodeStems.slice(node.edges.filter(e => e.type != "Missing").length > 0 ? 1 : 0).reverse()) {
                console.log("terminate " + terminatedStem + "@" + stems[terminatedStem].next);
                if (terminatedStem == stems.length - 1) {                    
                    stems.splice(terminatedStem, 1);
                    console.log(`${node.revision.change_id.prefix}/${node.revision.commit_id.prefix} stems, terminated: `, stems.map(s => {return { head: s.next }}));
                }
            }
                        
            // XXX look back up the graph to update edge coordinates - this requires filtering later when children aren't actually present
            let children = graph.filter(n2 => n2.ancestors.find(p => p.id.prefix == node.revision.commit_id.prefix));
            for (let child of children) {                
                let parent = child.ancestors.find(p => p.id.prefix == node.revision.commit_id.prefix);
                if (parent) {
                    parent.row = ixNode;
                    parent.column = nodeStem;
                }
            }

            // create row
            graph.push({
                row: ixNode,
                column: nodeStem,
                skip_columns: nodeStem >= stems.length ? 0 : stems.length - nodeStem - 1,
                id: node.revision.change_id,
                ancestors: node.edges.flatMap<GraphEdge>(e => {
                    switch (e.type) {
                        case "Direct":
                            return [{ row: -1, column: -1, branchEarly: false, id: e, hasElisions: false}];
                        case "Indirect":
                            return [{ row: -1, column: -1, branchEarly: false, id: e, hasElisions: true}];
                        case "Missing":
                            return [];
                    }
                }),
                desc: node.revision.description.lines[0],
                refs: node.revision.branches.filter(r => !(r.remote != null && r.is_synced && node.revision.branches.find(r2 => r2.remote == null && r2.name == r.name))),
                is_working_copy: $repo_status?.working_copy?.prefix == node.revision.commit_id.prefix,
                has_conflict: node.revision.has_conflict,
                select: () => ($change_content = node.revision),
            });

            // determine next set of stems by iterating parents
            for (let ixNode = 0; ixNode < node.edges.length; ixNode++) {
                let edge = node.edges[ixNode];
                console.log("stem for edge", edge);
                if (edge.type != "Missing") {
                    let edgeAttached = false;
                    for (let ixStem = 0; ixStem < stems.length; ixStem++) {
                        if (stems[ixStem].next == node.revision.commit_id.prefix) {
                            stems[ixStem].prev = stems[ixStem].next;
                            stems[ixStem].next = edge.prefix; 
                            edgeAttached = true; // continue current stem
                            console.log("continue " + edge.prefix);
                        } else if (stems[ixStem].next == edge.prefix && stems[ixStem].prev == node.revision.commit_id.prefix) {
                            edgeAttached = true; // continue another stem
                            console.log("attach " + edge.prefix);
                        }
                    }

                    // create a new stem
                    if (!edgeAttached) {
                        stems.push({ next: edge.prefix, prev: node.revision.commit_id.prefix });
                        console.log("push " + edge.prefix);
                    }
                }
            }
            
            console.log(`${node.revision.change_id.prefix}/${node.revision.commit_id.prefix} stems, after: `, stems.map(s => {return { head: s.next }}));
        }

        return graph;
    }

    function checkEdges(edges: GraphEdge[]) {
        return edges.filter(e => {
            if (e.row == -1) {
                console.log("bad edge", e);
                return false;
            } else {
                return true;
            }
        });
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

            <foreignObject class="row-html" x={node.skip_columns * 18 + 18} y="0" height="30" style="width: calc(100% - {(node.column + node.skip_columns) * 18 + 3}px)">
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
        {#each checkEdges(node.ancestors) as parent}
            <GraphLine has_elisions={parent.hasElisions} c1={node.column} c2={parent.column} r1={node.row} r2={parent.row} />
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