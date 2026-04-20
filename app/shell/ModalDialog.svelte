<script lang="ts">
    import { createEventDispatcher, onMount } from "svelte";

    interface $$Events {
        cancel: CustomEvent<void>;
        default: CustomEvent<void>;
    }

    export let title: string;
    export let error: boolean = false;

    let dispatch = createEventDispatcher();

    onMount(() => {
        document.addEventListener("keydown", onKeyDown);
        return () => {
            document.removeEventListener("keydown", onKeyDown);
        };
    });

    function onKeyDown(event: KeyboardEvent) {
        if (event.key == "Escape") {
            dispatch("cancel");
        } else if (event.key == "Enter") {
            dispatch("default");
        }
    }
</script>

<div id="dialog-chrome" role="dialog" aria-modal="true">
    <h3 id="dialog-header" class:error>{title}</h3>

    <div id="dialog-content">
        <slot />
    </div>

    <div id="dialog-commands">
        <slot name="commands" />
    </div>
</div>

<style>
    #dialog-chrome {
        --modal-padding: 12px;

        grid-area: 2/2/2/2;

        background: var(--gg-colors-background);
        border-radius: var(--gg-components-radiusLg);
        border: var(--gg-components-borderDialog);
        box-shadow: var(--gg-shadows-shadowLg);
        padding: var(--modal-padding);

        display: grid;
        grid-template-columns: 1fr;
        grid-template-rows: 30px auto 30px;
        grid-template-areas: 
            "header"
            "content"
            "commands";
    }

    #dialog-header {
        font-family: var(--gg-text-familyUi);
        grid-area: header;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    #dialog-content {
        grid-area: content;
        padding: 6px 0;
        display: grid;
        gap: 6px;
        grid-template-columns: auto 1fr;
        align-items: center;
    }

    #dialog-content > :global(:nth-child(even)) {
        justify-self: start;
    }

    #dialog-content :global(select),
    #dialog-content :global(input) {
        height: 30px;
        min-width: 30px;

        font-family: var(--gg-text-familyUi);
        font-size: 14px;
    }
    #dialog-content :global(select),
    #dialog-content :global(input[type="text"]),
    #dialog-content :global(input[type="password"]) {
        min-width: 180px;
    }
    #dialog-content :global(input[type="url"]) {
        min-width: 360px;
    }
    #dialog-content :global(input[type="checkbox"]) {
        vertical-align: middle;
    }

    #dialog-commands {
        grid-area: commands;
        display: flex;
        align-items: end;
        justify-content: end;
        gap: 6px;
    }

    .error {
        color: var(--gg-colors-error);
    }
</style>
