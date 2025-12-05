<script lang="ts" module>
    export interface List {
        getSize(): number;
        getSelection(): number;
        selectRow(row: number): void;
        editRow(row: number): void;
    }
</script>

<script lang="ts">
    import { onMount } from "svelte";
    import type { Snippet } from "svelte";

    import type { Operand } from "../messages/Operand";

    let { list, type, descendant, clientHeight = $bindable(0), clientWidth = $bindable(0), scrollTop = $bindable(0), children }: {
        list: List;
        type: Operand["type"];
        descendant: string | undefined;
        clientHeight: number;
        clientWidth: number;
        scrollTop: number;
        children?: Snippet;
    } = $props();

    let activedescendant = $derived(`${type}-${descendant}`);
    let box: HTMLElement;
    let pollFrame: number;

    onMount(() => {
        pollFrame = requestAnimationFrame(pollScroll);
        return () => {
            if (pollFrame) cancelAnimationFrame(pollFrame);
        };
    });

    function pollScroll() {
        if (box && box.scrollTop !== scrollTop) {
            scrollTop = box.scrollTop;
        }

        pollFrame = requestAnimationFrame(pollScroll);
    }

    function onKeyDown(event: KeyboardEvent) {
        if (list.getSize() == 0) {
            return;
        }

        let index: number;
        let pageRows: number;
        switch (event.key) {
            case "ArrowUp":
                event.preventDefault();
                index = list.getSelection();
                if (index > 0) {
                    onSelect(index - 1);
                }
                break;

            case "ArrowDown":
                event.preventDefault();
                index = list.getSelection();
                if (index != -1 && list.getSize() > index + 1) {
                    onSelect(index + 1);
                }
                break;

            case "PageUp":
                event.preventDefault();
                index = list.getSelection();
                pageRows = Math.round(box.clientHeight / 30);
                index = Math.max(index - pageRows, 0);
                onSelect(index);
                break;

            case "PageDown":
                event.preventDefault();
                index = list.getSelection();
                pageRows = Math.round(box.clientHeight / 30);
                index = Math.min(index + pageRows, list.getSize() - 1);
                onSelect(index);
                break;

            case "Home":
                event.preventDefault();
                onSelect(0);
                break;

            case "End":
                event.preventDefault();
                onSelect(list.getSize() - 1);
                break;

            case "Enter":
                list.editRow(list.getSelection());
        }
    }

    function onSelect(index: number) {
        box.focus();

        list.selectRow(index);

        let y = index * 30;
        if (box.scrollTop + box.clientHeight < y + 30) {
            box.scrollTo({
                left: 0,
                top: y - box.clientHeight + 30,
                behavior: "smooth",
            });
        } else if (box.scrollTop > y) {
            box.scrollTo({
                left: 0,
                top: y,
                behavior: "smooth",
            });
        }
    }
</script>

<ol
    class="listbox"
    role="listbox"
    aria-label="{type} List"
    aria-multiselectable="false"
    aria-activedescendant={activedescendant}
    tabindex="0"
    bind:this={box}
    bind:clientHeight
    bind:clientWidth
    onkeydown={onKeyDown}>
    {@render children?.()}
</ol>

<style>
    .listbox {
        overflow-x: hidden;
        overflow-y: auto;
        scrollbar-color: var(--ctp-text) var(--ctp-crust);
        display: grid;
        outline: none;
    }

    .listbox:focus-visible {
        outline: 2px solid var(--ctp-lavender);
        border-radius: 3px;
    }

    .listbox::-webkit-scrollbar {
        width: 6px;
    }

    .listbox::-webkit-scrollbar-thumb {
        background-color: var(--ctp-text);
        border-radius: 6px;
    }

    .listbox::-webkit-scrollbar-track {
        background-color: var(--ctp-crust);
    }
</style>
