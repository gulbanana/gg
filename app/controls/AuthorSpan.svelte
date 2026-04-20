<script lang="ts">
    import type { RevAuthor } from "../messages/RevAuthor";
    export let author: RevAuthor;
</script>

<span class="author" title={author.email}>
    {author.name}
</span>

<style>
    .author {
        color: var(--gg-colors-foregroundMuted);
        white-space: nowrap;
        pointer-events: auto;
    }
</style>
