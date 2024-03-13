<section>
    <div class="header">
        <slot name="header" />
    </div>
    <div class="body">
        <slot name="body" />
    </div>
</section>

<style>
    section {
        display: grid;
        grid-template-rows: 40px 1fr;
        grid-template-columns: 100%;
        overflow: hidden;
    }

    .header {
        border-bottom: 1px solid var(--ctp-overlay0);
        padding: 6px 6px 3px 6px;
    }

    .body {
        padding: 3px 6px 6px 6px;
        display: grid;
        overflow: hidden;
    }
</style>
