<script lang="ts" generics="T">
    import type { IPC } from "./ipc";

    export let ipc: IPC<T>;

    interface $$Slots {
        wait: {};
        error: { message: string };
        default: { value: T };
    }
</script>

{#if ipc.type == "wait"}
    <slot name="wait" />
{:else if ipc.type == "error"}
    <slot name="error" message={ipc.message}>
        <span class="red">{ipc.message}</span>
    </slot>
{:else}
    <slot value={ipc.value} />
{/if}

<style>
    .red {
        color: #d20f39;
    }
</style>
