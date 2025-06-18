<script lang="ts">
    import ActionWidget from "../controls/ActionWidget.svelte";
    import Icon from "../controls/Icon.svelte";
    import IdSpan from "../controls/IdSpan.svelte";
    import { mutate } from "../ipc";
    import type { Operand } from "../messages/Operand";
    import type { GitFetch } from "../messages/GitFetch";
    import type { GitPush } from "../messages/GitPush";
    import type { UndoOperation } from "../messages/UndoOperation";
    import type { RichHint } from "../mutators/BinaryMutator";
    import BinaryMutator from "../mutators/BinaryMutator";
    import { currentSource, currentTarget, hasModal, repoConfigEvent, repoStatusEvent } from "../stores";
    import BranchSpan from "../controls/BranchSpan.svelte";

    export let target: boolean;

    let dropHint: RichHint | null = null;
    let maybe = false;

    $: setDropHint($currentSource, $currentTarget);

    function setDropHint(source: Operand | null, target: Operand | null) {
        maybe = false;
        if (source) {
            if (target) {
                let canDrop = new BinaryMutator(source, target).canDrop();
                if (canDrop.type == "yes") {
                    dropHint = canDrop.hint;
                    return;
                } else if (canDrop.type == "maybe") {
                    dropHint = [canDrop.hint];
                    maybe = true;
                    return;
                }
            }

            let canDrag = BinaryMutator.canDrag(source);
            if (canDrag.type == "yes") {
                dropHint = canDrag.hint;
                return;
            }
        }

        dropHint = null;
    }

    function onUndo() {
        mutate<UndoOperation>("undo_operation", null);
    }

    function onPush(remote: string) {
        mutate<GitPush>("git_push", { type: "AllBookmarks", remote_name: remote });
    }

    function onFetch(remote: string) {
        mutate<GitFetch>("git_fetch", { type: "AllBookmarks", remote_name: remote });
    }
</script>

{#if !dropHint}
    <div id="status-bar" class="repo-bar" inert={$hasModal}>
        <div class="substatus">
            <span id="status-workspace">
                {$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.absolute_path : "No workspace"}
            </span>
        </div>
        <div id="status-remotes" class="substatus">
            {#if $repoConfigEvent?.type == "Workspace"}
                {#each $repoConfigEvent.git_remotes as remote}
                    <div class="substatus">
                        <ActionWidget tip="git push (all bookmarks)" onClick={() => onPush(remote)}>
                            <Icon name="upload-cloud" />
                        </ActionWidget>
                        <span>{remote}</span>
                        <ActionWidget tip="git fetch" onClick={() => onFetch(remote)}>
                            <Icon name="download-cloud" />
                        </ActionWidget>
                    </div>
                {/each}
            {/if}
        </div>
        <div id="status-operation" class="substatus">
            <span>
                {$repoConfigEvent?.type != "Workspace"
                    ? ""
                    : ($repoStatusEvent?.operation_description ?? "no operation")}
            </span>
            <ActionWidget tip="undo latest operation" onClick={onUndo} disabled={$repoConfigEvent?.type != "Workspace"}>
                <Icon name="rotate-ccw" /> Undo
            </ActionWidget>
        </div>
    </div>
{:else}
    <div id="status-bar" class="drag-bar" class:target class:maybe>
        <div>
            {#each dropHint as run, i}
                {#if typeof run == "string"}
                    <span>{run}{i == dropHint.length - 1 ? "." : ""}</span>
                {:else if run.type == "LocalBookmark" || run.type == "RemoteBookmark"}
                    <span><BranchSpan ref={run} /></span>
                {:else}
                    <span><IdSpan id={run} />{i == dropHint.length - 1 ? "." : ""}</span>
                {/if}
            {/each}
        </div>
    </div>
{/if}

<style>
    #status-bar {
        grid-area: footer;
        padding: 0 6px;
        gap: 6px;
        align-items: center;
    }

    .repo-bar {
        display: grid;
        grid-template-columns: minmax(120px, max-content) 1fr minmax(120px, max-content);
    }

    .drag-bar {
        display: flex;
        justify-content: center;
    }

    .substatus {
        height: 100%;
        display: flex;
        align-items: center;
        gap: 6px;
        white-space: nowrap;
    }

    .substatus > span {
        height: 21px;
    }

    #status-remotes {
        justify-content: space-evenly;
    }

    #status-operation {
        height: 100%;
        padding: 0 3px;
        justify-content: end;
        min-width: 0;
    }

    #status-operation > span {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    #status-workspace {
        white-space: nowrap;
        direction: rtl;
        text-align: left;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .target {
        background: var(--ctp-flamingo);
        color: black;
    }

    .maybe {
        background: transparent;
        color: var(--ctp-peach);
    }
</style>
