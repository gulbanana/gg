<script lang="ts">
    import ActionWidget from "../controls/ActionWidget.svelte";
    import Icon from "../controls/Icon.svelte";
    import { mutate } from "../ipc";
    import type { GitFetch } from "../messages/GitFetch";
    import type { GitPush } from "../messages/GitPush";
    import type { UndoOperation } from "../messages/UndoOperation";
    import type { RepoConfig } from "../messages/RepoConfig";
    import { ignoreToggled, hasModal, selectionHeaders } from "../stores";
    import RevisionMutator from "../mutators/RevisionMutator";

    export let config: RepoConfig;

    $: workspace = config.type === "Workspace" ? config : null;
    $: remotes = workspace?.git_remotes ?? [];
    $: defaultRemote = remotes.includes("origin") ? "origin" : remotes[0] ?? "";
    $: headers = $selectionHeaders ?? [];
    $: mutator = headers.length > 0 ? new RevisionMutator(headers, $ignoreToggled) : null;
    $: newest = headers[0] ?? null;
    $: oldest = headers.length > 0 ? headers[headers.length - 1] : null;
    $: newestImmutable = newest?.is_immutable && !$ignoreToggled;
    $: oldestImmutable = oldest?.is_immutable && !$ignoreToggled;

    let pushDropdown = false;
    let fetchDropdown = false;

    function onPush(remote_name: string) {
        pushDropdown = false;
        mutate<GitPush>(
            "git_push",
            { refspec: { type: "AllBookmarks", remote_name }, input: null },
            { operation: `Pushing to ${remote_name}...` },
        );
    }

    function onFetch(remote_name: string) {
        fetchDropdown = false;
        mutate<GitFetch>(
            "git_fetch",
            { refspec: { type: "AllBookmarks", remote_name }, input: null },
            { operation: `Fetching from ${remote_name}...` },
        );
    }

    function onUndo() {
        mutate<UndoOperation>("undo_operation", null);
    }

    function closeDropdowns(event: MouseEvent) {
        pushDropdown = false;
        fetchDropdown = false;
    }
</script>

<svelte:window on:click={closeDropdowns} />

<div class="toolbar" inert={$hasModal}>
    {#if workspace}
        <!-- Push / Fetch -->
        <div class="toolbar-group">
            {#if remotes.length > 0}
                <div class="split-button">
                    <button class="split-main" title="Push to {defaultRemote}" on:click|stopPropagation={() => onPush(defaultRemote)}>
                        <Icon name="upload-cloud" />
                    </button>
                    {#if remotes.length > 1}
                        <button class="split-chevron" title="Choose remote" on:click|stopPropagation={() => { pushDropdown = !pushDropdown; fetchDropdown = false; }}>
                            <Icon name="chevron-down" />
                        </button>
                        {#if pushDropdown}
                            <div class="split-dropdown">
                                {#each remotes as remote}
                                    <button on:click|stopPropagation={() => onPush(remote)}>Push to {remote}</button>
                                {/each}
                            </div>
                        {/if}
                    {/if}
                </div>

                <div class="split-button">
                    <button class="split-main" title="Fetch from {defaultRemote}" on:click|stopPropagation={() => onFetch(defaultRemote)}>
                        <Icon name="download-cloud" />
                    </button>
                    {#if remotes.length > 1}
                        <button class="split-chevron" title="Choose remote" on:click|stopPropagation={() => { fetchDropdown = !fetchDropdown; pushDropdown = false; }}>
                            <Icon name="chevron-down" />
                        </button>
                        {#if fetchDropdown}
                            <div class="split-dropdown">
                                {#each remotes as remote}
                                    <button on:click|stopPropagation={() => onFetch(remote)}>Fetch from {remote}</button>
                                {/each}
                            </div>
                        {/if}
                    {/if}
                </div>
            {/if}
        </div>

        <div class="toolbar-separator"></div>

        <!-- Revision actions -->
        <div class="toolbar-group">
            {#if mutator && newest}
                <ActionWidget
                    secondary
                    tip="Edit (make working copy)"
                    onClick={mutator.onEdit}
                    disabled={newestImmutable || newest.is_working_copy}>
                    <Icon name="edit-2" />
                </ActionWidget>
            {/if}
            {#if mutator}
                <ActionWidget secondary tip="New (create child)" onClick={mutator.onNewChild}>
                    <Icon name="plus-square" />
                </ActionWidget>
            {/if}
        </div>

        <div class="toolbar-separator"></div>

        <!-- Squash / Restore -->
        <div class="toolbar-group">
            {#if mutator && oldest}
                <ActionWidget
                    secondary
                    tip="Squash (move changes to parent)"
                    onClick={mutator.onSquash}
                    disabled={oldestImmutable || oldest.parent_ids.length != 1}>
                    <Icon name="upload" />
                </ActionWidget>
            {/if}
            {#if mutator && newest}
                <ActionWidget
                    secondary
                    tip="Restore (copy changes from parent)"
                    onClick={mutator.onRestore}
                    disabled={newestImmutable || newest.parent_ids.length != 1 || headers.length > 1}>
                    <Icon name="download" />
                </ActionWidget>
            {/if}
        </div>

        <div class="toolbar-separator"></div>

        <!-- Undo -->
        <div class="toolbar-group">
            <ActionWidget secondary tip="Undo latest operation" onClick={onUndo}>
                <Icon name="rotate-ccw" />
            </ActionWidget>
        </div>
    {/if}
</div>

<style>
    .toolbar {
        grid-area: toolbar;
        display: flex;
        align-items: center;
        gap: 2px;
        padding: 0 8px;
        background: var(--gg-colors-surfaceDeep);
        border-bottom: 1px solid var(--gg-colors-surfaceAlt);
        font-family: var(--gg-text-familyUi);
    }

    .toolbar-group {
        display: flex;
        align-items: center;
        gap: 2px;
        padding: 0 4px;
    }

    .toolbar-separator {
        width: 1px;
        height: 20px;
        background: var(--gg-colors-surfaceStrong);
        flex-shrink: 0;
    }

    .toolbar :global(button) {
        box-shadow: none;
        border: none;
        background: transparent;
        color: var(--gg-colors-foreground);
        padding: 4px;
        height: 28px;
        width: 28px;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .toolbar :global(button:not(:disabled):hover) {
        background: var(--gg-colors-surfaceAlt);
        box-shadow: none;
    }

    .toolbar :global(button:disabled) {
        background: transparent;
        color: var(--gg-colors-outlineStrong);
    }

    /* split button */
    .split-button {
        position: relative;
        display: flex;
        align-items: center;
        pointer-events: auto;
    }

    .split-main {
        pointer-events: auto;
        cursor: pointer;
    }

    .split-chevron {
        pointer-events: auto;
        cursor: pointer;
        width: 16px !important;
        padding: 4px 0 !important;
    }

    .split-chevron :global(svg) {
        width: 12px;
        height: 12px;
    }

    .split-dropdown {
        pointer-events: auto;
        position: absolute;
        top: 100%;
        left: 0;
        z-index: 100;
        background: var(--gg-colors-background);
        border: 1px solid var(--gg-colors-surfaceStrong);
        border-radius: var(--gg-components-radiusSm);
        box-shadow: var(--gg-shadows-shadowMd);
        min-width: 140px;
        padding: 2px 0;
    }

    .split-dropdown button {
        width: 100% !important;
        height: auto !important;
        padding: 6px 12px !important;
        font-size: 13px;
        text-align: left;
        justify-content: start !important;
        white-space: nowrap;
    }

    .split-dropdown button:hover {
        background: var(--gg-colors-surface) !important;
    }
</style>
