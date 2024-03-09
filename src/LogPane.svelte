<script lang="ts">
    import type { RepoStatus } from "./messages/RepoStatus.js";
    import type { LogPage } from "./messages/LogPage.js";
    import type { LogNode } from "./messages/LogNode.js";
    import type { RevId } from "./messages/RevId.js";
    import type { RevHeader } from "./messages/RevHeader.js";
    import { command, event } from "./ipc.js";
    import Bound from "./Bound.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";

    export let query: string;

    const repo_status = event<RepoStatus>("gg://repo/status");
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

        desc: string;
        is_conflict: boolean;
        is_working_copy: boolean;
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
                desc: n.revision.description.lines[0],
                is_working_copy: $repo_status.working_copy.prefix == n.revision.commit_id.prefix,
                is_conflict: n.revision.has_conflict,
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

                        <foreignObject class="row-html" x="18" y="0" height="30">
                            <div class="row-text" class:conflict={node.is_conflict}>
                                <code>
                                    <IdSpan type="change" id={node.id} />
                                </code>
                                <span class="row-desc">
                                    {node.desc}
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
    
    input {
        font-family: var(--stack-code);
        font-size: 14px;
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
    }

    .row-text.conflict {
        background: repeating-linear-gradient(
            120deg,
            transparent 0px,
            transparent 12px,
            var(--ctp-surface0) 12px,
            var(--ctp-surface0) 15px
        );
    }

    .row-desc {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
</style>
