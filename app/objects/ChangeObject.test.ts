import { describe, it, expect, beforeEach, afterEach, vi, type MockInstance } from "vitest";
import { render } from "@testing-library/svelte";
import { get } from "svelte/store";
import { setupMocks, cleanupMocks } from "../mocks";
import type { RevChange } from "../messages/RevChange";

let mockChange: RevChange = {
    kind: "Modified",
    path: { repo_path: "some/dir/file.txt", relative_path: "some/dir/file.txt" },
    has_conflict: false,
    hunks: [],
};

describe("ChangeObject", () => {
    let writeText: MockInstance;

    beforeEach(() => {
        setupMocks();
        writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    });

    afterEach(async () => {
        writeText.mockRestore();
        await cleanupMocks();
    });

    it("renders the file path and a copy path button", async () => {
        const { default: ChangeObject } = await import("./ChangeObject.svelte");

        const { container } = render(ChangeObject, {
            props: { headers: null, change: mockChange, selected: false },
        });

        expect(container.textContent).toContain("some/dir/file.txt");
        expect(container.querySelector("button[title='copy path']")).not.toBeNull();
    });

    it("clicking the copy path button copies the relative path", async () => {
        const { default: ChangeObject } = await import("./ChangeObject.svelte");

        const { container } = render(ChangeObject, {
            props: { headers: null, change: mockChange, selected: false },
        });

        let button = container.querySelector<HTMLButtonElement>("button[title='copy path']");
        button?.click();

        expect(writeText).toHaveBeenCalledExactlyOnceWith("some/dir/file.txt");
    });

    it("clicking the copy path button does not select the change", async () => {
        const { default: ChangeObject } = await import("./ChangeObject.svelte");
        const { changeSelectEvent } = await import("../stores");

        const { container } = render(ChangeObject, {
            props: { headers: null, change: mockChange, selected: false },
        });

        let button = container.querySelector<HTMLButtonElement>("button[title='copy path']");
        button?.click();

        expect(get(changeSelectEvent)).toBeUndefined();
    });
});
