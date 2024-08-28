<script lang="ts">
    import { createEventDispatcher, onMount } from "svelte";
    import type { InputResponse } from "../messages/InputResponse";
    import type { InputField } from "../messages/InputField";
    import ActionWidget from "../controls/ActionWidget.svelte";
    import ModalDialog from "./ModalDialog.svelte";
    import SelectWidget from "../controls/SelectWidget.svelte";

    interface $$Events {
        response: CustomEvent<InputResponse>;
    }

    export let title: string;
    export let detail: String;
    export let fields: InputField[];

    let dispatch = createEventDispatcher();

    onMount(() => {
        document.getElementById(`field-${fields[0].label}`)?.focus();
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
            if (field.choices.length == 0) {
                let input = document.getElementById(`field-${field.label}`) as HTMLInputElement;
                responseFields[field.label] = input.value;
            } else {
                let input = document.getElementById(`field-${field.label}`) as HTMLSelectElement;
                responseFields[field.label] = input.value;
            }
        }

        dispatch("response", {
            cancel: false,
            fields: responseFields,
        });
    }
</script>

<ModalDialog {title} on:cancel={onCancel} on:default={onEnter}>
    {#if detail != ""}
        <p>{detail}</p>
    {/if}
    {#each fields as field}
        <label for="field-{field.label}">{field.label}:</label>
        {#if field.choices.length > 0}
            <SelectWidget
                id="field-{field.label}"
                options={field.choices.map((c) => {
                    return { label: c, value: c };
                })}
                value={field.choices[0]} />
        {:else if field.label == "Password"}
            <input id="field-{field.label}" type="password" />
        {:else}
            <input id="field-{field.label}" type="text" />
        {/if}
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

    label:first-child {
        margin-top: 1em;
    }

    :last-of-type {
        margin-bottom: 1em;
    }
</style>
