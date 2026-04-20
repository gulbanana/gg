<script lang="ts">
    import IdSpan from "../controls/IdSpan.svelte";
    import { mutate } from "../ipc";
    import type { Operand } from "../messages/Operand";
    import type { GitFetch } from "../messages/GitFetch";
    import type { GitPush } from "../messages/GitPush";
    import type { RichHint } from "../mutators/BinaryMutator";
    import BinaryMutator from "../mutators/BinaryMutator";
    import { ignoreToggled, currentSource, currentTarget, hasModal, repoConfigEvent, repoStatusEvent } from "../stores";
    import { isTauri, trigger } from "../ipc";
    import ToggleWidget from "../controls/ToggleWidget.svelte";
    import BookmarkSpan from "../controls/BookmarkSpan.svelte";

    export let target: boolean;

    let dropHint: RichHint | null = null;
    let maybe = false;

    $: setDropHint($currentSource, $currentTarget);
    $: if (isTauri()) trigger("set_modifier_state", { alt: $ignoreToggled });

    function setDropHint(source: Operand | null, target: Operand | null) {
        maybe = false;
        if (source) {
            let mutator = new BinaryMutator(source, target, $ignoreToggled);
            if (target) {
                let canDrop = mutator.canDrop();
                if (canDrop.type == "yes") {
                    dropHint = canDrop.hint;
                    return;
                } else if (canDrop.type == "maybe") {
                    dropHint = [canDrop.hint];
                    maybe = true;
                    return;
                }
            }

            let canDrag = mutator.canDrag();
            if (canDrag.type == "yes") {
                dropHint = canDrag.hint;
                return;
            }
        }

        dropHint = null;
    }

    function onPush(remote_name: string) {
        mutate<GitPush>(
            "git_push",
            { refspec: { type: "AllBookmarks", remote_name }, input: null },
            { operation: `Pushing to ${remote_name}...` },
        );
    }

    function onFetch(remote_name: string) {
        mutate<GitFetch>(
            "git_fetch",
            { refspec: { type: "AllBookmarks", remote_name }, input: null },
            { operation: `Fetching from ${remote_name}...` },
        );
    }
</script>

{#if !dropHint}
    <div id="status-bar" class="repo-bar" inert={$hasModal}>
        <div class="substatus">
            <ToggleWidget tip="ignore immutability" bind:checked={$ignoreToggled} safe on="shield-off" off="shield" />
            <span id="status-workspace">
                {$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.absolute_path : "No workspace"}
            </span>
        </div>
        <div id="status-operation" class="substatus">
            <span>
                {$repoConfigEvent?.type != "Workspace"
                    ? ""
                    : "Last operation: " + ($repoStatusEvent?.operation_description ?? "none")}
            </span>
        </div>
    </div>
{:else}
    <div id="status-bar" class="drag-bar" class:target class:maybe>
        <div>
            {#each dropHint as run, i}
                {#if typeof run == "string"}
                    <span>{run}{i == dropHint.length - 1 ? "." : ""}</span>
                {:else if run.type == "LocalBookmark" || run.type == "RemoteBookmark"}
                    <span><BookmarkSpan ref={run} /></span>
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
        font-family: var(--gg-text-familyUi);
        font-size: var(--gg-text-sizeMd);
        gap: 6px;
        align-items: center;
    }

    .repo-bar {
        display: grid;
        grid-template-columns: 1fr auto;
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
        max-width: 40vw;
    }

    #status-workspace {
        white-space: nowrap;
    }

    .target {
        background: var(--gg-colors-accent);
        color: black;
    }

    .maybe {
        background: transparent;
        color: var(--gg-colors-highlight);
    }
</style>
