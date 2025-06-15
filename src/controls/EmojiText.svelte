<script lang="ts">
    import { gitmojis } from "gitmojis";

    export let text: string;

    const emojiMap = new Map();
    gitmojis.forEach((gitmoji) => {
        emojiMap.set(gitmoji.code, gitmoji.emoji);
    });

    function renderEmoji(text: string): string {
        if (!text) return text;

        return text.replace(/:([a-zA-Z0-9_+-]+):/g, (match, code) => {
            const emoji = emojiMap.get(`:${code}:`);
            return emoji || match;
        });
    }

    $: renderedText = renderEmoji(text);
</script>

<span>{renderedText}</span>
