import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render } from "@testing-library/svelte";
import type { RevsResult } from "./messages/RevsResult";
import { setupMocks, cleanupMocks } from "./mocks";

function createMockRevs(): Extract<RevsResult, { type: "Detail" }> {
    let mockId = {
        change: {
            type: "ChangeId" as const,
            hex: "abc123def456",
            prefix: "abc",
            rest: "123def456",
            offset: null,
            is_divergent: false,
        },
        commit: {
            type: "CommitId" as const,
            hex: "deadbeef1234",
            prefix: "dead",
            rest: "beef1234",
        },
    };

    return {
        type: "Detail",
        set: {
            from: mockId,
            to: mockId,
        },
        headers: [
            {
                id: mockId,
                description: { lines: ["Test commit message"] },
                author: {
                    email: "test@example.com",
                    name: "Test User",
                    timestamp: "2024-01-15T12:00:00Z",
                },
                has_conflict: false,
                is_working_copy: false,
                working_copy_of: null,
                is_immutable: false,
                refs: [],
                parent_ids: [],
            },
        ],
        parents: [],
        changes: [],
        conflicts: [],
    };
}

describe("RevisionPane", () => {
    beforeEach(() => {
        setupMocks();
    });

    afterEach(async () => {
        await cleanupMocks();
    });

    it("renders revision details with mocked data", async () => {
        const { default: RevisionPane } = await import("./RevisionPane.svelte");

        let mockRevs = createMockRevs();
        const { container } = render(RevisionPane, {
            props: {
                revs: mockRevs,
            },
        });

        // should display the change id
        expect(container.textContent).toContain("abc123de");

        // should display the commit id
        expect(container.textContent).toContain("deadbeef");

        // should display the description in the textarea
        let textarea = container.querySelector("textarea");
        expect(textarea).not.toBeNull();
        expect(textarea?.value).toBe("Test commit message");

        // should display the author
        expect(container.textContent).toContain("Test User");
    });
});
