<!-- XXX duplicates a lot of dispatch and enablement logic from menu.rs, but it's not easy to share... -->
<script lang="ts">
    import type { Operand } from "../messages/Operand";
    import type { RevHeader } from "../messages/RevHeader";
    import type { StoreRef } from "../messages/StoreRef";
    import { selectionHeaders } from "../stores";
    import { get } from "svelte/store";
    import RevisionMutator from "../mutators/RevisionMutator";
    import ChangeMutator from "../mutators/ChangeMutator";
    import RefMutator from "../mutators/RefMutator";

    export let operand: Operand;
    export let x: number;
    export let y: number;
    export let onClose: () => void;

    function onClick(action: string) {
        if (operand.type === "Revision" || operand.type === "Revisions") {
            new RevisionMutator(get(selectionHeaders)).handle(action);
        } else if (operand.type === "Change") {
            new ChangeMutator(operand.headers, operand.path, operand.hunk).handle(action);
        } else if (operand.type === "Ref") {
            new RefMutator(operand.ref).handle(action);
        }
        onClose();
    }

    function onDismiss() {
        onClose();
    }

    function onKeyDown(event: KeyboardEvent) {
        if (event.key === "Escape") {
            onClose();
        }
    }

    function isRevisionEnabled(headers: RevHeader[]) {
        const isSingleton = headers.length == 1;
        const anyImmutable = headers.some((h) => h.is_immutable);
        const hasSingleParent = headers[headers.length - 1]?.parent_ids.length == 1;

        return {
            new_child: true,
            new_parent: !anyImmutable && hasSingleParent,
            edit: isSingleton && !anyImmutable && !headers[0]?.is_working_copy,
            revert: true,
            duplicate: true,
            abandon: !anyImmutable,
            squash: !anyImmutable && hasSingleParent,
            restore: isSingleton && !anyImmutable && hasSingleParent,
            branch: isSingleton,
        };
    }

    function isChangeEnabled(headers: RevHeader[]) {
        const anyImmutable = headers.some((h) => h.is_immutable);
        const hasSingleParent = headers[headers.length - 1]?.parent_ids.length == 1;

        return {
            squash: !anyImmutable && hasSingleParent,
            restore: !anyImmutable && hasSingleParent,
        };
    }

    function isRefEnabled(ref: StoreRef) {
        return {
            track: ref.type === "RemoteBookmark" && !ref.is_tracked,
            untrack:
                (ref.type === "LocalBookmark" && ref.tracking_remotes.length > 0) ||
                (ref.type === "RemoteBookmark" && !ref.is_synced && ref.is_tracked && !ref.is_absent),
            push_all:
                (ref.type === "LocalBookmark" && ref.tracking_remotes.length > 0) ||
                (ref.type === "RemoteBookmark" && ref.is_tracked && ref.is_absent),
            push_single: ref.type === "LocalBookmark" && ref.potential_remotes > 0,
            fetch_all:
                (ref.type === "LocalBookmark" && ref.tracking_remotes.length > 0) ||
                (ref.type === "RemoteBookmark" && (!ref.is_tracked || !ref.is_absent)),
            fetch_single: ref.type === "LocalBookmark" && ref.available_remotes > 0,
            rename: ref.type === "LocalBookmark",
            delete: !(ref.type === "RemoteBookmark" && ref.is_absent && ref.is_tracked),
        };
    }

    $: revisionEnabled =
        operand.type === "Revision" || operand.type === "Revisions" ? isRevisionEnabled($selectionHeaders) : null;
    $: changeEnabled = operand.type === "Change" ? isChangeEnabled(operand.headers) : null;
    $: refEnabled = operand.type === "Ref" ? isRefEnabled(operand.ref) : null;

    // clamp to viewport
    let menuElement: HTMLDivElement;
    $: if (menuElement) {
        const rect = menuElement.getBoundingClientRect();
        const viewportWidth = window.innerWidth;
        const viewportHeight = window.innerHeight;

        if (x + rect.width > viewportWidth) {
            menuElement.style.left = `${viewportWidth - rect.width - 8}px`;
        }
        if (y + rect.height > viewportHeight) {
            menuElement.style.top = `${viewportHeight - rect.height - 8}px`;
        }
    }
</script>

<svelte:window on:click={onDismiss} />

<div
    class="context-menu"
    style="left: {x}px; top: {y}px;"
    role="menu"
    tabindex="0"
    on:click|stopPropagation
    on:keydown={onKeyDown}
    bind:this={menuElement}>
    {#if (operand.type === "Revision" || operand.type === "Revisions") && revisionEnabled}
        <button disabled={!revisionEnabled.new_child} on:click={() => onClick("new_child")}>New child</button>
        <button disabled={!revisionEnabled.new_parent} on:click={() => onClick("new_parent")}
            >New inserted parent</button>
        <hr />
        <button disabled={!revisionEnabled.edit} on:click={() => onClick("edit")}>Edit as working copy</button>
        <button disabled={!revisionEnabled.revert} on:click={() => onClick("revert")}
            >Revert into working copy</button>
        <button disabled={!revisionEnabled.duplicate} on:click={() => onClick("duplicate")}>Duplicate</button>
        <button disabled={!revisionEnabled.abandon} on:click={() => onClick("abandon")}>Abandon</button>
        <hr />
        <button disabled={!revisionEnabled.squash} on:click={() => onClick("squash")}>Squash into parent</button>
        <button disabled={!revisionEnabled.restore} on:click={() => onClick("restore")}>Restore from parent</button>
        <hr />
        <button disabled={!revisionEnabled.branch} on:click={() => onClick("branch")}>Create bookmark...</button>
    {:else if operand.type === "Change" && changeEnabled}
        <button disabled={!changeEnabled.squash} on:click={() => onClick("squash")}>Squash into parent</button>
        <button disabled={!changeEnabled.restore} on:click={() => onClick("restore")}>Restore from parent</button>
    {:else if operand.type === "Ref" && refEnabled}
        <button disabled={!refEnabled.track} on:click={() => onClick("track")}>Track</button>
        <button disabled={!refEnabled.untrack} on:click={() => onClick("untrack")}>Untrack</button>
        <hr />
        <button disabled={!refEnabled.push_all} on:click={() => onClick("push-all")}>Push</button>
        <button disabled={!refEnabled.push_single} on:click={() => onClick("push-single")}>Push to remote...</button>
        <button disabled={!refEnabled.fetch_all} on:click={() => onClick("fetch-all")}>Fetch</button>
        <button disabled={!refEnabled.fetch_single} on:click={() => onClick("fetch-single")}
            >Fetch from remote...</button>
        <hr />
        <button disabled={!refEnabled.rename} on:click={() => onClick("rename")}>Rename...</button>
        <button disabled={!refEnabled.delete} on:click={() => onClick("delete")}>Delete</button>
    {/if}
</div>

<style>
    .context-menu {
        position: fixed;
        z-index: 1000;
        background: var(--ctp-surface0);
        border: 1px solid var(--ctp-overlay0);
        border-radius: 3px;
        box-shadow: 2px 2px var(--ctp-text);
    }

    hr {
        border: none;
        border-top: 1px solid var(--ctp-overlay0);
        margin: 2px 0;
    }

    button {
        width: 100%;
        display: block;
        border: none;
        padding: 4px 12px;

        text-align: left;
        background: none;
        color: var(--ctp-text);
        font-family: var(--stack-industrial);

        &:disabled {
            color: var(--ctp-overlay0);
        }

        &:not(:disabled) {
            cursor: pointer;
            &:hover {
                background: var(--ctp-flamingo);
                color: buttontext;
            }
        }
    }
</style>
