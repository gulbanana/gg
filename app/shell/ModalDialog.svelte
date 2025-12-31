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
        grid-area: 2/2/2/2;

        background: var(--ctp-mantle);
        border-radius: 9px;
        border: 3px solid var(--ctp-overlay1);

        display: grid;
        grid-template-columns: 30px 1fr 33px;
        grid-template-rows: 30px auto 30px;
    }

    #dialog-header {
        margin-top: 6px;
        padding: 0 15px;
        grid-area: 1/2/2/2;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    #dialog-content {
        grid-area: 2/2/2/2;
        display: grid;
        grid-template-columns: auto 1fr;
        align-items: baseline;
        gap: 3px 6px;
    }

    #dialog-content > :global(:nth-child(even)) {
        justify-self: start;
    }

    #dialog-content :global(select),
    #dialog-content :global(input) {
        height: 30px;
        min-width: 30px;

        font-family: var(--stack-code);
        font-size: 14px;
    }
    #dialog-content :global(select),
    #dialog-content :global(input[type="text"]),
    #dialog-content :global(input[type="password"]) {
        min-width: 180px;
    }
    #dialog-content :global(input[type="checkbox"]) {
        vertical-align: middle;
    }

    #dialog-commands {
        margin-right: 3px;
        grid-area: 3/1/3/4;
        display: flex;
        align-items: center;
        justify-content: end;
        gap: 6px;
    }

    .error {
        color: var(--ctp-red);
    }
</style>
