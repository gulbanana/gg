<script lang="ts">
    import { createEventDispatcher } from "svelte";
    import ModalDialog from "./ModalDialog.svelte";
    import type Settings from "./Settings";

    export let settings: Settings;

    const dispatch = createEventDispatcher<{
        save: Settings;
        cancel: void;
    }>();

    // create a local copy of the settings for editing
    let localSettings: Settings = {
        markUnpushedBranches: settings.markUnpushedBranches,
        fontSize: settings.fontSize,
    };

    // font size input validation
    let fontSizeInput = localSettings.fontSize.toString();
    let fontSizeError = "";

    // validate font size input
    function validateFontSize(value: string): boolean {
        const num = parseInt(value);
        if (isNaN(num)) {
            fontSizeError = "Please enter a valid number";
            return false;
        }
        if (num < 8) {
            fontSizeError = "Font size cannot be less than 8px";
            return false;
        }
        if (num > 30) {
            fontSizeError = "Font size cannot be greater than 30px";
            return false;
        }
        fontSizeError = "";
        return true;
    }

    // handle font size input change
    function handleFontSizeInput(event: Event) {
        const target = event.target as HTMLInputElement;
        fontSizeInput = target.value;

        if (validateFontSize(fontSizeInput)) {
            localSettings.fontSize = parseInt(fontSizeInput);
        }
    }

    // handle input box blur correction
    function handleFontSizeBlur() {
        if (!validateFontSize(fontSizeInput)) {
            // if input is invalid, reset to current valid value
            fontSizeInput = localSettings.fontSize.toString();
            fontSizeError = "";
        }
    }

    function handleSave() {
        // validate one more time before saving
        if (validateFontSize(fontSizeInput)) {
            localSettings.fontSize = parseInt(fontSizeInput);
            dispatch("save", localSettings);
        }
    }

    function handleCancel() {
        dispatch("cancel");
    }

    // preview font size change in real time
    $: previewStyle = `font-size: ${localSettings.fontSize}px;`;
</script>

<ModalDialog title="Options" on:cancel={handleCancel} on:default={handleSave}>
    <div class="options-content">
        <div class="option-group">
            <div class="option-item">
                <label for="fontSize">Font Size (px):</label>
                <div class="input-container">
                    <input
                        id="fontSize"
                        type="number"
                        min="8"
                        max="30"
                        step="1"
                        bind:value={fontSizeInput}
                        on:input={handleFontSizeInput}
                        on:blur={handleFontSizeBlur}
                        class:error={fontSizeError} />
                    {#if fontSizeError}
                        <div class="error-message">{fontSizeError}</div>
                    {/if}
                </div>
            </div>

            <div class="size-hints">
                <span class="hint">Font size range: 8-30 px</span>
            </div>

            <div class="preview-area">
                <h3>Preview:</h3>
                <div class="preview-text" style={previewStyle}>This is a preview of the font size.</div>
            </div>
        </div>
    </div>

    <div slot="commands">
        <button type="button" on:click={handleCancel}>Cancel</button>
        <button type="button" on:click={handleSave} class="primary" disabled={!!fontSizeError}>Save</button>
    </div>
</ModalDialog>

<style>
    .options-content {
        padding: 16px;
        min-width: 400px;
        max-width: 500px;
    }

    .option-group {
        margin-bottom: 24px;
    }

    .option-group h3 {
        margin-bottom: 12px;
        color: var(--ctp-text);
        font-size: 16px;
        font-weight: 600;
    }

    .option-item {
        display: flex;
        align-items: flex-start;
        gap: 12px;
        margin-bottom: 12px;
    }

    .option-item label {
        min-width: 120px;
        color: var(--ctp-subtext1);
        margin-top: 6px;
    }

    .input-container {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 4px;
    }

    input[type="number"] {
        width: 100px;
        padding: 6px 8px;
        border-radius: 4px;
        background: var(--ctp-base);
        color: var(--ctp-text);
        border: 1px solid var(--ctp-overlay0);
        font-size: 14px;
    }

    input[type="number"]:focus {
        outline: none;
        border-color: var(--ctp-blue);
        box-shadow: 0 0 0 2px var(--ctp-blue, rgba(137, 180, 250, 0.3));
    }

    input[type="number"].error {
        border-color: var(--ctp-red);
    }

    input[type="number"].error:focus {
        border-color: var(--ctp-red);
        box-shadow: 0 0 0 2px rgba(243, 139, 168, 0.3);
    }

    .error-message {
        color: var(--ctp-red);
        font-size: 12px;
        margin-top: 2px;
    }

    .size-hints {
        display: flex;
        flex-direction: column;
        gap: 4px;
        margin-bottom: 16px;
        padding-left: 132px;
    }

    .hint {
        font-size: 12px;
        color: var(--ctp-subtext0);
    }

    .preview-area {
        margin-top: 16px;
        padding: 12px;
        background: var(--ctp-surface0);
        border-radius: 6px;
        border: 1px solid var(--ctp-overlay0);
    }

    .preview-text {
        color: var(--ctp-text);
        line-height: 1.5;
        padding: 8px;
        background: var(--ctp-base);
        border-radius: 4px;
        border: 1px solid var(--ctp-overlay0);
    }

    button {
        padding: 8px 16px;
        border-radius: 4px;
        border: 1px solid var(--ctp-overlay0);
        background: var(--ctp-surface0);
        color: var(--ctp-text);
        cursor: pointer;
        transition: all 0.2s ease;
    }

    button:hover:not(:disabled) {
        background: var(--ctp-surface1);
        border-color: var(--ctp-overlay1);
    }

    button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    button.primary {
        background: var(--ctp-blue);
        color: var(--ctp-crust);
        border-color: var(--ctp-blue);
    }

    button.primary:hover:not(:disabled) {
        background: var(--ctp-sapphire);
        border-color: var(--ctp-sapphire);
    }
</style>
