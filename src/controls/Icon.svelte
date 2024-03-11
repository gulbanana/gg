<script lang="ts">
    import sprite from "feather-icons/dist/feather-sprite.svg";
    export let name: string;
    export let state: "add" | "change" | "remove" | null = null;
</script>

<svg
    class="feather"
    class:add={state == "add"}
    class:change={state == "change"}
    class:remove={state == "remove"}
    viewBox="0 0 24 24">
    <use href="{sprite}#{name}" />
</svg>

<style>
    .feather {
        pointer-events: none;
        height: 16px;
        width: 16px;
        fill: none;
        stroke: currentColor;
        stroke-linecap: round;
        stroke-linejoin: round;
        stroke-width: 2;
    }

    .add {
        stroke: var(--ctp-green);
    }

    .change {
        color: var(--ctp-blue);
    }

    .remove {
        color: var(--ctp-red);
    }
</style>
