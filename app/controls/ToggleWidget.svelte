<script lang="ts">
    import { dragOverWidget, hasModal } from "../stores";
    import Icon from "./Icon.svelte";

    export let tip: string = "";
    export let checked: boolean;
    export let safe: boolean = false;
    export let secondary: boolean = false;
    export let disabled: boolean = false;
    export let on: string;
    export let off: string;

    $: isDisabled = disabled || (!safe && $hasModal);

    function toggle() {
        if (!isDisabled) {
            checked = !checked;
        }
    }
</script>

<button
    {disabled}
    class:safe
    class:secondary
    class:checked
    on:click={toggle}
    on:dragenter={dragOverWidget}
    on:dragover={dragOverWidget}
    title={isDisabled ? "" : tip}>
    <Icon name={checked ? on : off} />
</button>

<style>
    button {
        height: 24px;
        font-size: 16px;
        padding: 1px 3px;

        outline: none;
        margin: 0;
        background: var(--ctp-flamingo);
        border-width: 1px;
        border-radius: 3px;
        border-color: var(--ctp-overlay0);
        box-shadow: 2px 2px var(--ctp-overlay0);

        font-family: var(--stack-industrial);
        display: flex;
        align-items: center;

        cursor: pointer;
    }

    button:not(:disabled) {
        &:hover {
            background: var(--ctp-maroon);
        }
        &:focus-visible {
            border-color: var(--ctp-lavender);
            border-width: 2px;
            padding: 0px 2px;
            text-decoration: underline;
        }
        &:active {
            margin: 1px 0px 0px 1px;
            padding: 1px 2px 0px 3px;
            box-shadow: 1px 1px var(--ctp-overlay0);
            &:focus-visible {
                padding: 1px 1px 0px 2px;
            }
        }
    }

    button.checked:not(:disabled) {
        margin: 1px 0px 0px 1px;
        padding: 1px 2px 0px 3px;
        box-shadow: 1px 1px var(--ctp-overlay0);
        &:focus-visible {
            padding: 1px 1px 0px 2px;
        }
    }

    button.safe {
        background: var(--ctp-sapphire);
        &:hover {
            background: var(--ctp-teal);
        }
        &:active {
            border-right-color: var(--ctp-teal);
            border-bottom-color: var(--ctp-teal);
        }

        &.secondary {
            color: var(--ctp-text);
            background: var(--ctp-base);
            &:hover {
                background: var(--ctp-overlay0);
            }
            &:active {
                border-right-color: var(--ctp-overlay0);
                border-bottom-color: var(--ctp-overlay0);
                /* border-top-color: var(--ctp-text);
                border-left-color: var(--ctp-text); */
            }
        }
    }

    button:disabled {
        background: var(--ctp-mantle);
        color: var(--ctp-overlay2);
    }
</style>
