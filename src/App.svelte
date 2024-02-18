<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { Event } from "@tauri-apps/api/event";
  import type { RevHeader } from "./messages/RevHeader.js";
  import type { RevDetail } from "./messages/RevDetail.js";
  import type { RevId } from "./messages/RevId.js";
  import type { RepoConfig } from "./messages/RepoConfig.js";
  import { call, delayInit, init } from "./ipc.js";
  import Bound from "./Bound.svelte";
  import IdSpan from "./IdSpan.svelte";
  import Icon from "./Icon.svelte";
  import Pane from "./Pane.svelte";
  import RevisionPane from "./RevisionPane.svelte";
  import type { RepoStatus } from "./messages/RepoStatus.js";

  let shell_repo = "";
  let shell_op = "";
  let shell_wc = "";
  let log_content = init<RevHeader[]>();
  let change_content = init<RevDetail>();
  let entered_query = "";
  let selected_change = "";

  async function load_repo(config: RepoConfig) {
    log_content = init();
    change_content = init();

    entered_query = config.default_revset;
    shell_repo = config.absolute_path;

    update_repo(config.status);

    await load_log();
  }

  function update_repo(status: RepoStatus) {
    shell_op = status.operation_description;
    shell_wc = status.working_copy.prefix;
  }

  async function load_log() {
    let fetch = call<RevHeader[]>("query_log", {
      revset: entered_query,
    });
    log_content = await Promise.race([fetch, delayInit<RevHeader[]>()]);
    log_content = await fetch;

    if (log_content.type == "data") {
      await load_change(log_content.value[0].commit_id);
    }
  }

  async function load_change(id: RevId) {
    selected_change = id.prefix;
    let fetch = call<RevDetail>("get_revision", {
      rev: id.prefix + id.rest,
    });

    change_content = await Promise.race([fetch, delayInit<RevDetail>()]);
    change_content = await fetch;
  }

  listen<RepoConfig>("gg://repo/config", (e) => load_repo(e.payload));
  listen<RepoStatus>("gg://repo/status", (e) => update_repo(e.payload));

  document.addEventListener("keydown", (event) => {
    if (event.key === "o" && event.ctrlKey) {
      event.preventDefault();
      invoke("forward_accelerator", { key: "o" });
    }
  });

  invoke("notify_window_ready");
</script>

<div id="shell">
  <Pane>
    <div slot="header" class="log-selector">
      <select>
        <option selected>revsets.log</option>
        <option>all()</option>
      </select>
      <input type="text" bind:value={entered_query} on:change={load_log} />
    </div>
    <div slot="body" class="log-commits">
      <Bound ipc={log_content} let:value>
        {#each value as change}
          <!-- svelte-ignore a11y-click-events-have-key-events -->
          <!-- svelte-ignore a11y-no-static-element-interactions -->
          <div
            class="change"
            class:selected={selected_change == change.commit_id.prefix}
            on:click={() => load_change(change.commit_id)}
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

  <Bound ipc={change_content} let:value>
    <RevisionPane rev={value} />
    <Pane slot="wait" />
  </Bound>

  <div id="status-bar">
    <span>{shell_repo}</span>
    <span />
    <span>{shell_op}</span>
    <button><Icon name="rotate-ccw" /> Undo</button>
  </div>
</div>

<style>
  #shell {
    width: 100vw;
    height: 100vh;

    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 26px;
    gap: 3px;

    background: var(--ctp-overlay0);
    color: var(--ctp-text);

    user-select: none;
  }

  #status-bar {
    grid-column: 1/3;
    background: var(--ctp-crust);
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 6px;
    padding: 0 3px;
  }

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
