import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
    TestContext,
    createContext,
    destroyContext,
    extractTestRepo,
    startGG,
} from "./harness.js";

// known commit IDs from the test repo (see src/worker/tests/mod.rs)
// main_bookmark: renamed c.txt
let MAIN_BOOKMARK = {
    change: { type: "ChangeId", hex: "wnpusytq", prefix: "wnpusytq", rest: "", offset: null, is_divergent: false },
    commit: { type: "CommitId", hex: "025843422c8f5374a4160fe79195b92d6ec3c6ee", prefix: "025843422c8f5374a4160fe79195b92d6ec3c6ee", rest: "" },
};

function revSet(id: typeof MAIN_BOOKMARK) {
    return { from: id, to: id };
}

describe("revert into working copy", () => {
    let ctx: TestContext;
    let serverUrl: string;

    async function apiMutate(command: string, mutation: unknown) {
        let response = await fetch(`${serverUrl}/api/mutate/${command}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                mutation,
                options: { ignore_immutable: false },
            }),
        });
        let body = await response.json();
        expect(response.ok, `${command} failed (${response.status}): ${JSON.stringify(body)}`).toBe(true);
        return body;
    }

    async function apiQuery(command: string, body: unknown) {
        let response = await fetch(`${serverUrl}/api/query/${command}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
        let result = await response.json();
        expect(response.ok, `${command} failed (${response.status}): ${JSON.stringify(result)}`).toBe(true);
        return result;
    }

    beforeEach(async () => {
        ctx = await createContext();
        extractTestRepo(ctx.tempDir);
        serverUrl = await startGG(ctx);
        await apiQuery("query_workspace", { path: null });
    });

    afterEach(async () => {
        await destroyContext(ctx);
    });

    it("reverting a commit backs out its changes from the working copy", async () => {
        // the default working copy is an empty child of main_bookmark.
        // reverting main_bookmark (which renamed c.txt) should apply the
        // reverse of that rename into the working copy.
        let result = await apiMutate("backout_revisions", { set: revSet(MAIN_BOOKMARK) });
        expect(result.type).toBe("Updated");

        // query the working copy - it should now have changes
        let log = await apiQuery("query_log", { revset: "@" });
        let wcHeader = log.rows[0].revision;
        expect(wcHeader.is_working_copy).toBe(true);

        let detail = await apiQuery("query_revisions", { set: revSet(wcHeader.id) });
        expect(detail.type).toBe("Detail");
        expect(detail.changes.length).toBeGreaterThan(0);
    });
});
