<script lang="ts">
    import sprite from "feather-icons/dist/feather-sprite.svg";
    export let name: string;
</script>

<svg class="feather" viewBox="0 0 24 24">
    <use href="{sprite}#{name}" />
</svg>

<style>
    .feather {
        height: 12px;
        width: 12px;
        fill: none;
        stroke: currentColor;
        stroke-linecap: round;
        stroke-linejoin: round;
        stroke-width: 2;
    }
</style>
