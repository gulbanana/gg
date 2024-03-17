<script lang="ts">
    import { createEventDispatcher, onMount } from "svelte";
    import type { InputResponse } from "../messages/InputResponse";
    import ActionWidget from "../controls/ActionWidget.svelte";
    import ModalDialog from "./ModalDialog.svelte";

    interface $$Events {
        response: CustomEvent<InputResponse>;
    }

    export let title: string;
    export let detail: String;
    export let fields: string[];

    let dispatch = createEventDispatcher();

    onMount(() => {
        document.getElementById(`field-${fields[0]}`)?.focus();
    });

    function onCancel() {
        dispatch("response", {
            cancel: true,
            fields: {},
        });
    }

    function onEnter() {
        let responseFields: Record<string, string> = {};
        for (let field of fields) {
            // XXX maybe use databinding instead
            let input = document.getElementById(`field-${field}`) as HTMLInputElement;
            responseFields[field] = input.value;
        }

        dispatch("response", {
            cancel: false,
            fields: responseFields,
        });
    }
</script>

<ModalDialog {title} on:cancel={onCancel} on:default={onEnter}>
    <p>{detail}</p>
    {#each fields as field}
        <label for={field}>{field}:</label>
        <input id="field-{field}" type={field == "Password" ? "password" : "text"} />
    {/each}
    <svelte:fragment slot="commands">
        <ActionWidget safe onClick={onEnter}>Enter</ActionWidget>
        <ActionWidget safe onClick={onCancel}>Cancel</ActionWidget>
    </svelte:fragment>
</ModalDialog>

<style>
    p {
        grid-column: 1/3;
        word-wrap: break-word;
    }

    :last-of-type {
        margin-bottom: 1em;
    }
</style>