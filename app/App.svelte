<script lang="ts">
    import { parseRoute } from "./route.js";
    import Shell from "./Shell.svelte";
    import LogPane from "./LogPane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import BoundQuery from "./controls/BoundQuery.svelte";
    import Pane from "./shell/Pane.svelte";
    import TitlebarInset from "./shell/TitlebarInset.svelte";
    import SetSpan from "./controls/SetSpan.svelte";

    let route = parseRoute();
    let leftFraction = 0.5; // 50/50 split
    let isDragging = false;

    let minFraction = 0.1;
    let maxFraction = 0.9;
    let keyboardStep = 0.05;

    function resize(fraction: number) {
        leftFraction = Math.max(minFraction, Math.min(maxFraction, fraction));
    }

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
        resize((e.clientX - rect.left) / (rect.width - 4)); // 4px is separator width
    }

    function onMouseUp() {
        isDragging = false;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
    }

    function onKeyDown(e: KeyboardEvent) {
        switch (e.key) {
            case "ArrowLeft":
                resize(leftFraction - keyboardStep);
                break;
            case "ArrowRight":
                resize(leftFraction + keyboardStep);
                break;
            case "Home":
                resize(minFraction);
                break;
            case "End":
                resize(maxFraction);
                break;
            default:
                return;
        }
        e.preventDefault();
    }
</script>

<Shell revsetOverride={route.type === "revision" ? route.revset : null} let:workspace let:selection>
    {#if route.type === "log"}
        {#key workspace.absolute_path}
            <LogPane query_choices={workspace.query_choices} latest_query={route.revset ?? workspace.latest_query} />
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
        <div class="two-pane" style="--left-fraction: {leftFraction}fr; --right-fraction: {1 - leftFraction}fr;">
            <TitlebarInset>
                {#key workspace.absolute_path}
                    <LogPane query_choices={workspace.query_choices} latest_query={workspace.latest_query} />
                {/key}
            </TitlebarInset>

            <!-- svelte-ignore a11y_no_interactive_element_to_noninteractive_role (a focusable separator is a widget, per the aria window splitter pattern) -->
            <button
                type="button"
                class="separator"
                role="separator"
                aria-label="Resize log pane"
                aria-orientation="vertical"
                aria-valuenow={Math.round(leftFraction * 100)}
                aria-valuemin={minFraction * 100}
                aria-valuemax={maxFraction * 100}
                on:mousedown={onMouseDown}
                on:keydown={onKeyDown}
                class:dragging={isDragging}>
            </button>

            <BoundQuery query={selection} let:data>
                {#if data.type == "Detail"}
                    <RevisionPane revs={data} />
                {:else}
                    <Pane titlebar>
                        <h2 slot="header">Not Found</h2>
                        <p slot="body">
                            Empty revision set <SetSpan set={data.set} />.
                        </p>
                    </Pane>
                {/if}
                <Pane titlebar slot="error" let:message>
                    <h2 slot="header">Error</h2>
                    <p slot="body">{message}</p>
                </Pane>
                <Pane titlebar slot="wait">
                    <h2 slot="header">Loading...</h2>
                </Pane>
            </BoundQuery>
        </div>
    {/if}
</Shell>

<style>
    .two-pane {
        display: grid;
        grid-template-columns: var(--left-fraction) 4px var(--right-fraction);
        height: 100%;
        overflow: hidden;
    }

    .separator {
        background: var(--ctp-overlay0);
        cursor: col-resize;
        user-select: none;
        width: 4px;
        margin-left: -2.5px;
        border: none;
        padding: 0;
    }

    .separator:hover {
        background: var(--ctp-overlay2);
    }

    .separator:focus-visible {
        outline: none;
    }

    .separator.dragging,
    .separator:focus-visible {
        background: var(--ctp-lavender);
    }
</style>
