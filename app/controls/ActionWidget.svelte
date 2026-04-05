<script lang="ts">
    import { dragOverWidget, hasModal } from "../stores";

    export let tip: string = "";
    export let onClick: (event: MouseEvent) => void;
    export let safe: boolean = false;
    export let secondary: boolean = false;
    export let primary: boolean = false;
    export let disabled: boolean = false;

    $: isDisabled = disabled || (!safe && $hasModal);
</script>

<button
    disabled={isDisabled}
    class:safe
    class:secondary
    class:primary
    on:click|stopPropagation={isDisabled ? undefined : onClick}
    on:dragenter={dragOverWidget}
    on:dragover={dragOverWidget}
    title={isDisabled ? "" : tip}>
    <slot />
</button>

<style>
    button {
        height: var(--gg-components-buttonHeight);
        font-size: 14px;
        padding: var(--gg-components-buttonPadding);

        outline: none;
        margin: 0;
        border: var(--gg-components-buttonBorder);
        border-radius: var(--gg-components-buttonRadius);
        box-shadow: var(--gg-components-buttonShadow);

        font-family: var(--gg-text-familyUi);
        display: flex;
        align-items: center;
        gap: 3px;

        cursor: pointer;
        transition: background var(--gg-components-transitionFast), box-shadow var(--gg-components-transitionFast), transform var(--gg-components-transitionFast);

        color: var(--gg-components-buttonForeground);
        background: var(--gg-components-buttonBackground);
    }

    button:not(:disabled) {
        &:hover {
            background: var(--gg-components-buttonHoverBackground);
            box-shadow: var(--gg-shadows-shadowMd);
        }
        &:focus-visible {
            border-color: var(--gg-colors-focusRing);
            border-width: 2px;
            padding: 0px 5px;
            text-decoration: underline;
        }
        &:active {
            margin: var(--active-margin);
            transform: var(--gg-components-buttonActiveTransform);
            box-shadow: var(--gg-components-buttonActiveShadow);
            &:focus-visible {
                padding: 1px 4px 0px 5px;
            }
        }
    }

    button.primary {
        color: var(--gg-colors-primaryContent);
        background: var(--gg-colors-primary);
        &:hover {
            background: var(--gg-components-buttonHoverBackground);
        }
    }

    button.secondary {
        color: var(--gg-components-buttonSecondaryForeground);
        background: var(--gg-components-buttonSecondaryBackground);
        &:hover {
            background: var(--gg-components-buttonSecondaryHoverBackground);
        }
    }

    button:disabled {
        background: var(--gg-components-buttonDisabledBackground);
        color: var(--gg-components-buttonDisabledForeground);
    }
</style>
