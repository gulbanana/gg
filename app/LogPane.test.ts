import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import type { LogPage } from "./messages/LogPage";
import { setupMocks, cleanupMocks } from "./mocks";

describe("LogPane", () => {
    beforeAll(() => {
        setupMocks((cmd, _args) => {
            if (cmd === "query_log") {
                let emptyPage: LogPage = { rows: [], has_more: false };
                return emptyPage;
            }
            return undefined;
        });
    });

    afterAll(async () => {
        await cleanupMocks();
    });

    it("renders loading state", async () => {
        const { default: LogPane } = await import("./LogPane.svelte");

        const { container } = render(LogPane, {
            props: {
                default_query: "all()",
                latest_query: "all()",
            },
        });

        expect(container.textContent).toContain("Loading");
    });

    it("renders empty log with mocked IPC", async () => {
        const { default: LogPane } = await import("./LogPane.svelte");

        const { container } = render(LogPane, {
            props: {
                default_query: "all()",
                latest_query: "all()",
            },
        });

        await waitFor(() => {
            expect(container.textContent).not.toContain("Loading");
        });
    });
});
