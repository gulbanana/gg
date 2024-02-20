<script lang="ts">
    import type { LogPage } from "./messages/LogPage.js";
    import type { RevHeader } from "./messages/RevHeader.js";
    import { command, event } from "./ipc.js";
    import Bound from "./Bound.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";
    import type { LogNode } from "./messages/LogNode.js";
    import type { RevId } from "./messages/RevId.js";

    export let query: string;

    const log_content = command<LogPage>("query_log");
    const change_content = event<RevHeader>("gg://change/select");

    let entered_query = query;

    load_log();

    async function load_log() {
        let log = await log_content.call({
            revset: entered_query,
        });

        if (log.type == "data" && log.value.nodes.length > 0) {
            $change_content = log.value.nodes[0].revision;
        }
    }

    interface Descendant {
        node: GraphNode;
        parents: Set<string>;
    }

    interface GraphNode {
        row: number;
        column: number;
        id: RevId;
        parents: GraphNode[];
        title: string;
        select: () => void;
    }

    function formatNodes(nodes: LogNode[]): GraphNode[] {
        let graph: GraphNode[] = [];

        let unresolved_parents = new Set<Descendant>();

        let row = 0;
        for (let n of nodes) {
            let ancestorOf: GraphNode[] = [];

            for (let p of unresolved_parents) {
                if (p.parents.delete(n.revision.commit_id.prefix)) {
                    ancestorOf.push(p.node);
                    if (p.parents.size == 0) {
                        unresolved_parents.delete(p);
                    }
                }
            }

            let node = {
                row: row++,
                column: unresolved_parents.size,
                id: n.revision.change_id,
                parents: [],
                title: n.revision.description.lines[0],
                select: () => ($change_content = n.revision),
            };

            graph.push(node);

            for (let descendant of ancestorOf) {
                descendant.parents.push(node);
            }

            let targets = n.edges.flatMap<string>((e) => {
                switch (e.type) {
                    case "Direct":
                        return [e.prefix];
                    case "Indirect":
                        return [e.prefix];
                    case "Missing":
                        return [];
                }
            });

            unresolved_parents.add({
                node,
                parents: new Set(targets),
            });
        }

        return graph;
    }
</script>

<Pane>
    <div slot="header" class="log-selector">
        <select>
            <option selected>revsets.log</option>
            <option>all()</option>
        </select>
        <input type="text" bind:value={entered_query} on:change={load_log} />
    </div>
    <div slot="body" class="log-commits">
        <Bound query={$log_content} let:data>
            <div slot="wait">Loading changes...</div>
            <svg
                class="graph"
                style="width: 100%; height: {data.nodes.length * 30}px;"
            >
                {#each formatNodes(data.nodes) as node}
                    <g
                        class="row"
                        transform="translate({node.column * 18} {node.row *
                            30})"
                    >
                        <rect
                            class="row-backdrop"
                            class:selected={$change_content?.change_id.prefix ==
                                node.id.prefix}
                            height="30"
                            width="100%"
                        />
                        <circle cx="9" cy="15" r="6" fill="none" />
                        <foreignObject x="18" y="0" width="100%" height="30">
                            <!-- svelte-ignore a11y-click-events-have-key-events -->
                            <!-- svelte-ignore a11y-no-static-element-interactions -->
                            <div class="change" on:click={node.select}>
                                <span class="change-line">
                                    <code>
                                        <IdSpan id={node.id} type="change" />
                                    </code>
                                    {node.title}
                                </span>
                            </div>
                        </foreignObject>
                    </g>
                {/each}
                {#each formatNodes(data.nodes) as node}
                    {#each node.parents as parent}
                        <line
                            x1={node.column * 18 + 9}
                            y1={node.row * 30 + 21}
                            x2={parent.column * 18 + 9}
                            y2={parent.row * 30 + 9}
                        />
                    {/each}
                {/each}
            </svg>
        </Bound>
    </div>
</Pane>

<style>
    .log-selector {
        height: 100%;
        display: grid;
        grid-template-columns: auto 1fr;
        gap: 3px;
    }

    .log-commits {
        overflow-x: hidden;
        overflow-y: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);
        display: grid;
        user-select: none;
    }

    svg {
        stroke: var(--ctp-text);
        fill: var(--ctp-text);
    }

    .row {
        cursor: pointer;
    }

    .row-backdrop {
        stroke: none;
        fill: none;
    }

    .row-backdrop:global(.selected) {
        fill: var(--ctp-base);
    }

    .change {
        height: 100%;
        display: flex;
        flex-direction: column;
        justify-content: center;
    }

    .change-line {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .change-line > code {
        margin: 0 6px;
    }

    input {
        font-family: var(--stack-code);
        font-size: 14px;
    }
</style>
