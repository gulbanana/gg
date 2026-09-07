<script lang="ts">
    import { parseRoute } from "./route.js";
    import Shell from "./Shell.svelte";
    import LogPane from "./LogPane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import BoundQuery from "./controls/BoundQuery.svelte";
    import Pane from "./shell/Pane.svelte";
    import SetSpan from "./controls/SetSpan.svelte";

    let route = parseRoute();
    let leftFraction = 0.5; // 50/50 split
    let isDragging = false;

    function onMouseDown(e: MouseEvent) {
        e.preventDefault();
        isDragging = true;
        document.addEventListener("mousemove", onMouseMove);
        document.addEventListener("mouseup", onMouseUp);
    }

    function onMouseMove(e: MouseEvent) {
        if (!isDragging) return;

        let container = document.querySelector(".two-pane") as HTMLElement;
        if (!container) return;

        let rect = container.getBoundingClientRect();
        let containerWidth = rect.width;
        let mouseX = e.clientX - rect.left;

        // Clamp between 20% and 80% of container width
        let minWidth = containerWidth * 0.05;
        let maxWidth = containerWidth * 0.95;
        let clampedX = Math.max(minWidth, Math.min(maxWidth, mouseX));

        // Calculate left pane fraction (convert pixel position to fraction)
        leftFraction = clampedX / (containerWidth - 4); // 4px is separator width
    }

    function onMouseUp() {
        isDragging = false;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
    }
</script>

<Shell revsetOverride={route.type === "revision" ? route.revset : null}
       let:workspace let:selection>
    {#if route.type === "log"}
        {#key workspace.absolute_path}
            <LogPane query_choices={workspace.query_choices}
                     latest_query={route.revset ?? workspace.latest_query} />
        {/key}
    {:else if route.type === "revision"}
        <BoundQuery query={selection} let:data>
            {#if data.type == "Detail"}
                <RevisionPane revs={data} />
            {:else}
                <Pane>
                    <h2 slot="header">Not Found</h2>
                    <p slot="body">
                        Empty revision set <SetSpan set={data.set} />.
                    </p>
                </Pane>
            {/if}
            <Pane slot="error" let:message>
                <h2 slot="header">Error</h2>
                <p slot="body">{message}</p>
            </Pane>
            <Pane slot="wait">
                <h2 slot="header">Loading...</h2>
            </Pane>
        </BoundQuery>
    {:else}
        <div class="two-pane" style="grid-template-columns: {leftFraction}fr 4px {1 - leftFraction}fr;">
            {#key workspace.absolute_path}
                <LogPane query_choices={workspace.query_choices}
                         latest_query={workspace.latest_query} />
            {/key}

            <div class="separator" on:mousedown={onMouseDown} class:dragging={isDragging}></div>

            <BoundQuery query={selection} let:data>
                {#if data.type == "Detail"}
                    <RevisionPane revs={data} />
                {:else}
                    <Pane>
                        <h2 slot="header">Not Found</h2>
                        <p slot="body">
                            Empty revision set <SetSpan set={data.set} />.
                        </p>
                    </Pane>
                {/if}
                <Pane slot="error" let:message>
                    <h2 slot="header">Error</h2>
                    <p slot="body">{message}</p>
                </Pane>
                <Pane slot="wait">
                    <h2 slot="header">Loading...</h2>
                </Pane>
            </BoundQuery>
        </div>
    {/if}
</Shell>

<style>
    .two-pane {
        display: grid;
        height: 100%;
        overflow: hidden;
    }

    .separator {
        background: var(--ctp-overlay0);
        cursor: col-resize;
        user-select: none;
        pointer-events: auto;
        width: 4px;
        margin-left: -2.5px;
    }

    .separator:hover {
        background: var(--ctp-surface0);
    }

    .separator.dragging {
        background: var(--ctp-blue);
    }
</style>
