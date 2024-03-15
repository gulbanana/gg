<script lang="ts">
    import type { RevId } from "./messages/RevId";
    import type { RevResult } from "./messages/RevResult";
    import type { RepoConfig } from "./messages/RepoConfig";
    import type { UndoOperation } from "./messages/UndoOperation";
    import type { Event } from "@tauri-apps/api/event";
    import { type Query, query, command, mutate, delay, onEvent } from "./ipc.js";
    import {
        currentMutation,
        currentContext,
        repoConfigEvent,
        repoStatusEvent,
        revisionSelectEvent,
        currentSource,
        currentTarget,
    } from "./stores.js";
    import BranchMutator from "./mutators/BranchMutator";
    import ChangeMutator from "./mutators/ChangeMutator";
    import RevisionMutator from "./mutators/RevisionMutator";
    import Pane from "./Pane.svelte";
    import RevisionPane from "./RevisionPane.svelte";
    import LogPane from "./LogPane.svelte";
    import BoundQuery from "./controls/BoundQuery.svelte";
    import Zone from "./objects/Zone.svelte";
    import StatusBar from "./StatusBar.svelte";
    import ModalOverlay from "./ModalOverlay.svelte";
    import ModalDialog from "./ModalDialog.svelte";

    let selection: Query<RevResult> = {
        type: "wait",
    };

    document.addEventListener("keydown", (event) => {
        if (event.key === "o" && event.ctrlKey) {
            event.preventDefault();
            command("forward_accelerator", { key: "o" });
        } else if (event.key == "escape") {
            currentMutation.set(null);
        }
    });

    document.body.addEventListener("click", () => currentContext.set(null), true);

    command("notify_window_ready");

    onEvent("gg://context/revision", mutateRevision);
    onEvent("gg://context/tree", mutateTree);
    onEvent("gg://context/branch", mutateBranch);

    $: if ($repoConfigEvent) loadRepo($repoConfigEvent);
    $: if ($repoStatusEvent && $revisionSelectEvent) loadChange($revisionSelectEvent.change_id);

    async function loadRepo(config: RepoConfig) {
        $revisionSelectEvent = undefined;
        if (config.type == "Workspace") {
            $repoStatusEvent = config.status;
        }
    }

    async function loadChange(id: RevId) {
        let fetch = await query<RevResult>("query_revision", {
            query: id.hex,
        });

        let rev = await Promise.race([fetch, delay<RevResult>()]);

        if (rev.type == "wait") {
            selection = rev;
            rev = await fetch;
        }

        if (rev.type == "data" && rev.value.type == "NotFound" && id.hex != $repoStatusEvent?.working_copy.hex) {
            return loadChange($repoStatusEvent?.working_copy!);
        }

        selection = rev;
    }

    function mutateRevision(event: Event<string>) {
        console.log(`mutateRevision(${event.payload})`, $currentContext);
        if ($currentContext?.type == "Revision") {
            new RevisionMutator($currentContext.header).handle(event.payload);
        }
        $currentContext = null;
    }

    function mutateTree(event: Event<string>) {
        console.log(`mutateTree(${event.payload})`, $currentContext);
        if ($currentContext?.type == "Change") {
            new ChangeMutator($currentContext.header, $currentContext.path).handle(event.payload);
        }
        $currentContext = null;
    }

    function mutateBranch(event: Event<string>) {
        console.log(`mutateBranch(${event.payload})`, $currentContext);
        if ($currentContext?.type == "Branch") {
            new BranchMutator($currentContext.header, $currentContext.name).handle(event.payload);
        }
        $currentContext = null;
    }
</script>

<Zone operand={{ type: "Repository" }} alwaysTarget let:target>
    <div id="shell" class={$repoConfigEvent?.type == "Workspace" ? $repoConfigEvent.theme : ""}>
        {#if $repoConfigEvent?.type == "Workspace"}
            {#key $repoConfigEvent.absolute_path}
                <LogPane default_query={$repoConfigEvent.default_query} latest_query={$repoConfigEvent.latest_query} />
            {/key}

            <div class="separator" />

            <BoundQuery query={selection} let:data>
                {#if data.type == "Detail"}
                    <RevisionPane rev={data} />
                {:else}
                    <Pane>
                        <h2 slot="header">Not Found</h2>
                        <p slot="body">Revset '{data.query}' is empty.</p>
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
        {:else if !$repoConfigEvent}
            <ModalOverlay>
                <ModalDialog title="Fatal Error" severe>
                    <p>Error communicating with backend. You may need to restart GG to continue.</p>
                </ModalDialog>
            </ModalOverlay>
        {:else}
            <ModalOverlay>
                <ModalDialog title={$repoConfigEvent.type == "NoWorkspace" ? "No Workspace Loaded" : "Internal Error"}>
                    <p>{$repoConfigEvent.error}.</p>
                    <p>Try opening a workspace from the Repository menu.</p>
                </ModalDialog>
            </ModalOverlay>
        {/if}

        <div class="separator" style="grid-area: 2/1/3/4" />

        <StatusBar {target} />

        {#if $currentMutation}
            <ModalOverlay>
                {#if $currentMutation.type == "data" && ($currentMutation.value.type == "InternalError" || $currentMutation.value.type == "PreconditionError")}
                    <ModalDialog title="Command Error" onClose={() => ($currentMutation = null)} severe>
                        <p>{$currentMutation.value.message}</p>
                    </ModalDialog>
                {:else if $currentMutation.type == "error"}
                    <ModalDialog title="IPC Error" onClose={() => ($currentMutation = null)} severe>
                        <p>{$currentMutation.message}</p>
                    </ModalDialog>
                {/if}
            </ModalOverlay>
        {/if}
    </div>
</Zone>

<style>
    #shell {
        width: 100vw;
        height: 100vh;

        display: grid;
        grid-template-columns: 1fr 3px 1fr;
        grid-template-rows: 1fr 3px 30px;
        grid-template-areas:
            "content content content"
            ". . ."
            "footer footer footer";

        background: var(--ctp-crust);
        color: var(--ctp-text);

        user-select: none;
    }

    .separator {
        background: var(--ctp-overlay0);
    }
</style>
