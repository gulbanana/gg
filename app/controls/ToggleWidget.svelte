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
        height: var(--gg-components-buttonHeight);
        font-size: 16px;
        padding: 1px 3px;

        outline: none;
        margin: 0;
        border-width: 1px;
        border-radius: var(--gg-components-radiusSm);
        border-color: var(--gg-colors-outline);
        box-shadow: var(--gg-shadows-shadowSm);

        background: var(--gg-colors-accent);
        color: black;

        font-family: var(--gg-text-familyUi);
        display: flex;
        align-items: center;

        cursor: pointer;
        transition: background var(--gg-components-transitionFast), box-shadow var(--gg-components-transitionFast), transform var(--gg-components-transitionFast);
    }

    button:not(:disabled) {
        &:hover {
            background: var(--gg-colors-accentHover);
            box-shadow: var(--gg-shadows-shadowMd);
        }
        &:focus-visible {
            border-color: var(--gg-colors-focusRing);
            border-width: 2px;
            padding: 0px 2px;
            text-decoration: underline;
        }
        &:active {
            margin: var(--active-margin);
            transform: var(--gg-components-buttonActiveTransform);
            box-shadow: var(--gg-components-buttonActiveShadow);
            &:focus-visible {
                padding: 1px 1px 0px 2px;
            }
        }
    }

    button.checked:not(:disabled) {
        margin: var(--active-margin);
        box-shadow: var(--gg-components-buttonActiveShadow);
        &:focus-visible {
            padding: 1px 1px 0px 2px;
        }
    }

    button.safe {
        background: var(--gg-colors-success);
        &:hover {
            background: var(--gg-colors-success);
        }
    }

    button.secondary {
        background: var(--gg-colors-surfaceStrong);
        &:hover {
            background: var(--gg-colors-foregroundSubtle);
        }
    }

    button:disabled {
        background: var(--gg-colors-surface);
        color: var(--gg-colors-foregroundSubtle);
    }
</style>
