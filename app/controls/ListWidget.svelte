<script lang="ts" context="module">
    export interface Selection {
        from: number;
        to: number;
    }

    export interface List {
        getSize(): number;
        getSelection(): Selection;
        selectRow(row: number): void;
        extendSelection(row: number): void;
        editRow(row: number): void;
    }
</script>

<script lang="ts">
    import { onMount } from "svelte";

    import type { Operand } from "../messages/Operand";

    interface $$Slots {
        default: {};
    }

    export let list: List;
    export let type: Operand["type"];
    export let descendant: string | undefined;
    export let clientHeight = 0;
    export let clientWidth = 0;
    export let scrollTop = 0;

    let activedescendant = `${type}-${descendant}`;
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

        let selection: Selection;
        let minIdx: number;
        let maxIdx: number;
        let pageRows: number;
        switch (event.key) {
            case "ArrowUp":
                event.preventDefault();
                selection = list.getSelection();
                if (event.shiftKey) {
                    if (selection.to > 0) {
                        onExtend(selection.to - 1);
                    }
                } else {
                    minIdx = Math.min(selection.from, selection.to);
                    if (minIdx > 0) {
                        onSelect(minIdx - 1);
                    }
                }
                break;

            case "ArrowDown":
                event.preventDefault();
                selection = list.getSelection();
                if (event.shiftKey) {
                    if (selection.to != -1 && list.getSize() > selection.to + 1) {
                        onExtend(selection.to + 1);
                    }
                } else {
                    maxIdx = Math.max(selection.from, selection.to);
                    if (maxIdx != -1 && list.getSize() > maxIdx + 1) {
                        onSelect(maxIdx + 1);
                    }
                }
                break;

            case "PageUp":
                event.preventDefault();
                selection = list.getSelection();
                minIdx = Math.min(selection.from, selection.to);
                pageRows = Math.round(box.clientHeight / 30);
                minIdx = Math.max(minIdx - pageRows, 0);
                onSelect(minIdx);
                break;

            case "PageDown":
                event.preventDefault();
                selection = list.getSelection();
                maxIdx = Math.max(selection.from, selection.to);
                pageRows = Math.round(box.clientHeight / 30);
                maxIdx = Math.min(maxIdx + pageRows, list.getSize() - 1);
                onSelect(maxIdx);
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
                selection = list.getSelection();
                if (selection.from == selection.to) {
                    list.editRow(selection.from);
                }
        }
    }

    function onSelect(index: number) {
        box.focus();
        list.selectRow(index);
        scrollToRow(index);
    }

    function onExtend(index: number) {
        box.focus();
        list.extendSelection(index);
        scrollToRow(index);
    }

    function scrollToRow(index: number) {
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
    on:keydown={onKeyDown}>
    <slot />
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
