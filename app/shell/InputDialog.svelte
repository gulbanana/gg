<script lang="ts">
    import { createEventDispatcher, onMount } from "svelte";
    import type { InputResponse } from "../messages/InputResponse";
    import type { InputField } from "../messages/InputField";
    import ActionWidget from "../controls/ActionWidget.svelte";
    import CheckWidget from "../controls/CheckWidget.svelte";
    import ModalDialog from "./ModalDialog.svelte";
    import SelectWidget from "../controls/SelectWidget.svelte";

    type FieldType = "text" | "password" | "select" | "check";

    function getType(field: InputField): FieldType {
        if (field.choices.length === 2 && field.choices.includes("true") && field.choices.includes("false")) {
            return "check";
        } else if (field.choices.length > 1) {
            return "select";
        } else if (field.label.toLowerCase().includes("password")) {
            return "password";
        } else {
            return "text";
        }
    }

    interface $$Events {
        response: CustomEvent<InputResponse | null>;
    }

    export let title: string;
    export let detail: String;
    export let fields: InputField[];

    let dispatch = createEventDispatcher();

    onMount(() => {
        document.getElementById(`field-${fields[0].label}`)?.focus();
    });

    function onCancel() {
        dispatch("response", null);
    }

    function onEnter() {
        let responseFields: Record<string, string> = {};
        for (let field of fields) {
            switch (getType(field)) {
                case "text":
                case "password":
                    let textInput = document.getElementById(`field-${field.label}`) as HTMLInputElement;
                    responseFields[field.label] = textInput.value;
                    break;
                case "select":
                    let selectInput = document.getElementById(`field-${field.label}`) as HTMLSelectElement;
                    responseFields[field.label] = selectInput.value;
                case "check":
                    let checkInput = document.getElementById(`field-${field.label}`) as HTMLInputElement;
                    responseFields[field.label] = checkInput.checked ? "true" : "false";
                    break;
            }
        }

        dispatch("response", {
            fields: responseFields,
        });
    }
</script>

<ModalDialog {title} on:cancel={onCancel} on:default={onEnter}>
    {#if detail != ""}
        <p>{detail}</p>
    {/if}
    {#each fields as field}
        <label for="field-{field.label}">{field.label}{field.label.endsWith(":") ? "" : ":"}</label>
        {#if getType(field) == "text"}
            <input
                id="field-{field.label}"
                type="text"
                autocapitalize="off"
                autocorrect="off"
                autocomplete="off"
                value={field.choices.length == 1 ? field.choices[0] : ""} />
        {:else if getType(field) == "password"}
            <input id="field-{field.label}" type="password" />
        {:else if getType(field) == "select"}
            <SelectWidget
                id="field-{field.label}"
                value={field.choices[0]}
                options={field.choices.map((c) => ({ label: c, value: c }))} />
        {:else if getType(field) == "check"}
            <span><CheckWidget id="field-{field.label}" checked={field.choices[0] == "true"} /></span>
        {/if}
    {/each}
    <div class="separator"></div>
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

    .separator {
        height: 1em;
    }
</style>
