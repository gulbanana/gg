<section>
    <header>
        <slot name="header" />
    </header>
    <article>
        <slot name="body" />
    </article>
</section>

<style>
    section {
        display: grid;
        grid-template-rows: 40px 1fr;
        grid-template-columns: 100%;
        overflow: hidden;
        background: var(--ctp-mantle);
    }

    header {
        border-bottom: 1px solid var(--ctp-overlay0);
        padding: 6px 6px 3px 6px;
    }

    article {
        padding: 3px 6px 6px 6px;
    }
</style>
