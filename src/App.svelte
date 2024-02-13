<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { call, init } from "./ipc.js";
  import type { RevHeader } from "./messages/RevHeader.js";
  import type { RevDetail } from "./messages/RevDetail.js";
  import type { RevId } from "./messages/RevId.js";
  import Bound from "./Bound.svelte";
  import IdSpan from "./IdSpan.svelte";
  import Icon from "./Icon.svelte";
  import Pane from "./Pane.svelte";
  import PathSpan from "./PathSpan.svelte";

  let log_content = init<RevHeader[]>();
  let change_content = init<RevDetail>();
  let selected_change = "";
  let selected_path = "";

  async function load_log() {
    log_content = await call<RevHeader[]>("load_log");
    change_content = init();
    if (log_content.type == "data") {
      await load_change(log_content.value[0].change_id);
    }
  }

  async function load_change(id: RevId) {
    selected_change = id.prefix;
    selected_path = "";
    change_content = await call<RevDetail>("load_change", {
      revision: id.prefix + id.rest,
    });
  }

  listen("gg://repo_loaded", load_log);
  load_log();

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
      <input
        type="text"
        value="..@ | ancestors(immutable_heads().., 2) | heads(immutable_heads())"
      />
    </div>
    <div slot="body" class="log-commits">
      <Bound ipc={log_content} let:value>
        {#each value as change}
          <!-- svelte-ignore a11y-click-events-have-key-events -->
          <!-- svelte-ignore a11y-no-static-element-interactions -->
          <div
            class="change"
            class:selected={selected_change == change.change_id.prefix}
            on:click={() => load_change(change.change_id)}
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
    <Pane>
      <h2 slot="header">
        <span>
          <IdSpan id={value.header.change_id} type="change" />
          /
          <IdSpan id={value.header.commit_id} type="commit" />
        </span>
        <button class="pin-commit"><Icon name="map-pin" /> Pin</button>
      </h2>

      <div slot="body" class="commit-body">
        <textarea spellcheck="false"
          >{value.header.description.lines.join("\n")}</textarea
        >
        <div class="author">
          <span>{value.header.author}</span>
          <span>{new Date(value.header.timestamp).toLocaleTimeString()}</span>
          <span></span>
          <button><Icon name="file-text" /> Describe</button>
        </div>
        <div class="diff">
          {#each value.diff as path}
            <!-- svelte-ignore a11y-click-events-have-key-events -->
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <div
              class="path"
              class:selected={selected_path == path.relative_path}
              on:click={() => (selected_path = path.relative_path)}
            >
              <PathSpan {path} />
            </div>
          {/each}
        </div>
        <div class="commands">
          <button>Abandon</button>
          <button>Squash</button>
          <button>Restore</button>
        </div>
      </div></Pane
    >
  </Bound>

  <div id="status-bar">
    <span>C:\Users\banana\Documents\code\gg</span>
    <span />
    <span>abandon commit d59a92df72aa220cdcc0dd0cfe6e7e02a0b35f28</span>
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

  #status-bar {
    grid-column: 1/3;
    background: var(--ctp-crust);
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 6px;
    padding: 0 3px;
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

  .author {
    color: var(--ctp-yellow);
    display: inline-block;
    width: 24ch;
  }

  .timestamp {
    color: var(--ctp-teal);
  }

  .diff {
    background: var(--ctp-mantle);
    border-radius: 6px;
    padding: 3px;
    display: flex;
    flex-direction: column;
  }
  .path {
    height: 24px;
    display: flex;
    align-items: center;
    cursor: pointer;
  }

  h2 {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  input {
    font-family: var(--stack-code);
    font-size: 14px;
  }

  textarea {
    border-radius: 6px;
    width: 100%;
    height: 5em;
  }

  .commit-body {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 3px;
  }

  .selected {
    background: var(--ctp-base);
  }

  .pin-commit {
    background: var(--ctp-sapphire);
  }

  .author {
    color: var(--ctp-subtext1);
    width: 100%;
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    gap: 6px;
  }

  .author > button {
    background: var(--ctp-peach);
  }

  .commands {
    display: flex;
    justify-content: end;
    gap: 6px;
  }

  .commands > button {
    background: var(--ctp-maroon);
  }
</style>
