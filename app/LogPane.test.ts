import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from "vitest";
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
                query_choices: { default: "all()" },
                latest_query: "all()",
            },
        });

        expect(container.textContent).toContain("Loading");
    });

    it("renders empty log with mocked IPC", async () => {
        const { default: LogPane } = await import("./LogPane.svelte");

        const { container } = render(LogPane, {
            props: {
                query_choices: { default: "all()" },
                latest_query: "all()",
            },
        });

        await waitFor(() => {
            expect(container.textContent).not.toContain("Loading");
        });
    });
});

describe("LogPane (query requests)", () => {
    let revsets: string[];

    beforeEach(() => {
        revsets = [];
        setupMocks((cmd, args) => {
            if (cmd === "query_log") {
                revsets.push(args.revset as string);
                let emptyPage: LogPage = { rows: [], has_more: false };
                return emptyPage;
            }
            return undefined;
        });
    });

    afterEach(async () => {
        await cleanupMocks();
    });

    it("a logQueryRequest repoints the log and shows up in the revset box", async () => {
        const { default: LogPane } = await import("./LogPane.svelte");
        const { logQueryRequest } = await import("./stores");
        const { get } = await import("svelte/store");
        logQueryRequest.set(null);

        const { container } = render(LogPane, {
            props: {
                query_choices: { default: "all()" },
                latest_query: "all()",
            },
        });

        await waitFor(() => expect(revsets).toEqual(["all()"]));

        logQueryRequest.set('files("a.txt")');

        await waitFor(() => expect(revsets).toEqual(["all()", 'files("a.txt")']));

        // the request is one-shot, and the new revset is editable in the header
        expect(get(logQueryRequest)).toBe(null);
        let input = container.querySelector("input")!;
        expect(input.value).toBe('files("a.txt")');
    });
});
