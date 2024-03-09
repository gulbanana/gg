<script lang="ts">
    import type { RevHeader } from "./messages/RevHeader.js";
    import type { RevId } from "./messages/RevId.js";
    import { command, event } from "./ipc.js";
    import Bound from "./Bound.svelte";
    import IdSpan from "./IdSpan.svelte";
    import Pane from "./Pane.svelte";

    export let query: string;

    const log_content = command<RevHeader[]>("query_log");
    const change_content = event<RevId>("gg://change/select");
          
    let entered_query = query;
  
    load_log();
    
    async function load_log() {
        let log = await log_content.call({
            revset: entered_query,
        })
    
        if (log.type == "data") {
            $change_content = log.value[0].commit_id;
        }
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
            <div slot="wait" class="change">
                Loading changes...
            </div>
            {#each data as change}
                <!-- svelte-ignore a11y-click-events-have-key-events -->
                <!-- svelte-ignore a11y-no-static-element-interactions -->
                <div
                    class="change"
                    class:selected={$change_content?.prefix == change.commit_id.prefix}
                    on:click={() => $change_content = change.commit_id}
                >
                    <span class="change-line">
                    <code>
                        <IdSpan id={change.change_id} type="change" />
                    </code>
                    {change.description.lines[0]}
                    </span>
                </div>
            {/each}
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
        overflow-y: scroll;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);
        display: flex;
        flex-direction: column;
        gap: 1em;
        user-select: none;
    }

    .selected {
        background: var(--ctp-base);
    }

    .change {
        display: flex;
        flex-direction: column;
        cursor: pointer;
        background: var(--ctp-mantle);
        border-radius: 3px;
    }

    .change-line {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    input {
        font-family: var(--stack-code);
        font-size: 14px;
    }
</style>