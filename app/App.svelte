<script lang="ts">
    import { parseRoute } from "./route.js";
    import Shell from "./Shell.svelte";
    import LogPane from "./LogPane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import BoundQuery from "./controls/BoundQuery.svelte";
    import Pane from "./shell/Pane.svelte";
    import SetSpan from "./controls/SetSpan.svelte";

    let route = parseRoute();
</script>

<Shell {route} revsetOverride={route.type === "revision" ? route.revset : null}
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
        <div class="two-pane">
            {#key workspace.absolute_path}
                <LogPane query_choices={workspace.query_choices}
                         latest_query={workspace.latest_query} />
            {/key}

            <div class="separator"></div>

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
        grid-template-columns: 1fr 3px 1fr;
        height: 100%;
        overflow: hidden;
    }

    .separator {
        background: var(--ctp-overlay0);
    }
</style>
