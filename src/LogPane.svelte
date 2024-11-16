<script lang="ts">
    import { onMount } from "svelte";
    import type { LogPage } from "./messages/LogPage.js";
    import type { LogRow } from "./messages/LogRow.js";
    import { query } from "./ipc.js";
    import { repoStatusEvent, revisionSelectEvent } from "./stores.js";
    import Pane from "./shell/Pane.svelte";
    import RevisionObject from "./objects/RevisionObject.svelte";
    import SelectWidget from "./controls/SelectWidget.svelte";
    import RevisionMutator from "./mutators/RevisionMutator.js";
    import { type EnhancedRow, default as GraphLog, type EnhancedLine } from "./GraphLog.svelte";
    import ListWidget, { type List } from "./controls/ListWidget.svelte";

    export let default_query: string;
    export let latest_query: string;

    const presets = [
        { label: "Default", value: default_query },
        { label: "Tracked Bookmarks", value: "@ | ancestors(bookmarks(), 5)" },
        {
            label: "Remote Bookmarks",
            value: "@ | ancestors(remote_bookmarks(), 5)",
        },
        { label: "All Revisions", value: "all()" },
    ];

    let choices: ReturnType<typeof getChoices>;
    let entered_query = latest_query;
    let graphRows: EnhancedRow[] | undefined;

    let logHeight = 0;
    let logWidth = 0;
    let logScrollTop = 0;

    // all these calculations are not efficient. probably doesn't matter
    let list: List = {
        getSize() {
            return graphRows?.length ?? 0;
        },
        getSelection() {
            return (
                graphRows?.findIndex((row) => row.revision.id.commit.hex == $revisionSelectEvent?.id.commit.hex) ?? -1
            );
        },
        selectRow(row: number) {
            $revisionSelectEvent = graphRows![row].revision;
        },
        editRow(row: number) {
            if (row != -1) {
                new RevisionMutator(graphRows![row].revision).onEdit();
            }
        },
    };

    onMount(() => {
        loadLog();
    });

    $: if (entered_query) choices = getChoices();
    $: if ($repoStatusEvent) reloadLog();

    function getChoices() {
        let choices = presets;
        for (let choice of choices) {
            if (entered_query == choice.value) {
                return choices;
            }
        }

        choices = [{ label: "Custom", value: entered_query }, ...presets];

        return choices;
    }

    async function loadLog() {
        let page = await query<LogPage>(
            "query_log",
            {
                revset: entered_query == "" ? "all()" : entered_query,
            },
            () => (graphRows = undefined),
        );

        if (page.type == "data") {
            graphRows = [];
            graphRows = addPageToGraph(graphRows, page.value.rows);

            if (page.value.rows.length > 0) {
                $revisionSelectEvent = page.value.rows[0].revision;
            }

            while (page.value.has_more) {
                let next_page = await query<LogPage>("query_log_next_page", null);
                if (next_page.type == "data") {
                    graphRows = addPageToGraph(graphRows, next_page.value.rows);
                    page = next_page;
                } else {
                    break;
                }
            }
        }
    }

    async function reloadLog() {
        let page = await query<LogPage>(
            "query_log",
            {
                revset: entered_query == "" ? "all()" : entered_query,
            },
            () => (graphRows = undefined),
        );

        if (page.type == "data") {
            graphRows = [];
            graphRows = addPageToGraph(graphRows, page.value.rows);

            while (page.value.has_more) {
                let next_page = await query<LogPage>("query_log_next_page", null);
                if (next_page.type == "data") {
                    graphRows = addPageToGraph(graphRows, next_page.value.rows);
                    page = next_page;
                } else {
                    break;
                }
            }
        }
    }

    // augment rows with all lines that pass through them
    let lineKey = 0;
    let passNextRow: EnhancedLine[] = [];
    function addPageToGraph(graph: EnhancedRow[], page: LogRow[]): EnhancedRow[] {
        for (let row of page) {
            let enhancedRow = row as EnhancedRow;
            for (let passingRow of passNextRow) {
                passingRow.parent = row.revision;
            }
            enhancedRow.passingLines = passNextRow;
            passNextRow = [];

            for (let line of enhancedRow.lines) {
                let enhancedLine = line as EnhancedLine;
                enhancedLine.key = lineKey++;

                if (line.type == "ToIntersection" || line.type == "ToMissing") {
                    // ToIntersection lines begin at their owning row, so they run from this row to the next one that we read (which may not be on the same page)
                    enhancedLine.child = row.revision;
                    enhancedRow.passingLines.push(enhancedLine);
                    passNextRow.push(enhancedLine);
                } else {
                    // other lines end at their owning row, so we need to add them to all previous rows and then this one
                    enhancedLine.parent = row.revision;
                    enhancedLine.child = graph[line.source[1]].revision;
                    for (let i = line.source[1]; i < line.target[1]; i++) {
                        graph[i].passingLines.push(enhancedLine);
                    }
                    enhancedRow.passingLines.push(enhancedLine);
                }
            }

            graph.push(enhancedRow);
        }

        return graph;
    }
</script>

<Pane>
    <div slot="header" class="log-selector">
        <SelectWidget options={choices} bind:value={entered_query} on:change={reloadLog}>
            <svelte:fragment let:option>{option.label}</svelte:fragment>
        </SelectWidget>
        <input type="text" bind:value={entered_query} on:change={reloadLog} />
    </div>

    <ListWidget
        slot="body"
        type="Revision"
        descendant={$revisionSelectEvent?.id.commit.prefix}
        {list}
        bind:clientHeight={logHeight}
        bind:clientWidth={logWidth}
        bind:scrollTop={logScrollTop}>
        {#if graphRows}
            <GraphLog
                containerHeight={logHeight}
                containerWidth={logWidth}
                scrollTop={logScrollTop}
                rows={graphRows}
                let:row>
                {#if row}
                    <RevisionObject
                        header={row.revision}
                        selected={$revisionSelectEvent?.id.commit.hex == row.revision.id.commit.hex} />
                {/if}
            </GraphLog>
        {:else}
            <div>Loading changes...</div>
        {/if}
    </ListWidget>
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
</style>
