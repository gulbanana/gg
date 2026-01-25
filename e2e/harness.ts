import { chromium, Browser, Page } from "@playwright/test";
import { spawn, ChildProcess, execSync } from "child_process";
import { mkdtemp, rm } from "fs/promises";
import { tmpdir } from "os";
import { join } from "path";
import { fileURLToPath } from "url";

export const PROJECT_ROOT = join(fileURLToPath(import.meta.url), "../..");
export const TEST_REPO_ZIP = join(PROJECT_ROOT, "res", "test-repo.zip");

export interface TestContext {
    browser: Browser;
    page: Page;
    ggProcess: ChildProcess | null;
    tempDir: string;
}

export async function createContext(): Promise<TestContext> {
    let browser = await chromium.launch({ headless: true });
    let page = await browser.newPage();
    let tempDir = await mkdtemp(join(tmpdir(), "gg-test-"));
    return { browser, page, tempDir, ggProcess: null };
}

export async function destroyContext(ctx: TestContext): Promise<void> {
    await ctx.page?.close();
    await ctx.browser?.close();

    if (ctx.ggProcess) {
        ctx.ggProcess.kill();
        ctx.ggProcess = null;
    }

    try {
        await rm(ctx.tempDir, { recursive: true, force: true });
    } catch {
        // ignore cleanup errors
    }
}

export function startGG(ctx: TestContext): Promise<string> {
    let ggProcess = spawn("cargo", ["run", "--", "web", ctx.tempDir, "--no-launch"], {
        cwd: PROJECT_ROOT,
        stdio: ["ignore", "pipe", "pipe"],
    });

    ctx.ggProcess = ggProcess;

    let serverUrl = new Promise<string>((resolve, reject) => {
        let stderrOutput = "";
        let stdoutOutput = "";

        ggProcess.stderr?.on("data", (data) => {
            stderrOutput += data.toString();
            let match = stderrOutput.match(/http:\/\/127\.0\.0\.1:\d+/);
            if (match) {
                resolve(match[0]);
            }
        });

        ggProcess.stdout?.on("data", (data) => {
            stdoutOutput += data.toString();
            let match = stdoutOutput.match(/http:\/\/127\.0\.0\.1:\d+/);
            if (match) {
                resolve(match[0]);
            }
        });

        ggProcess.on("error", (err) => {
            reject(new Error(`Failed to start gg: ${err.message}`));
        });

        ggProcess.on("exit", (code) => {
            if (code !== null && code !== 0) {
                reject(new Error(`cargo run exited with code ${code}\nstderr: ${stderrOutput}\nstdout: ${stdoutOutput}`));
            }
        });

        // timeout after 120 seconds to allow for compilation
        setTimeout(
            () => reject(new Error(`cargo run timed out\nstderr: ${stderrOutput}\nstdout: ${stdoutOutput}`)),
            120000,
        );
    });

    return serverUrl;
}

export function extractTestRepo(destDir: string): void {
    if (process.platform === "win32") {
        // Windows: use tar (available since Windows 10 1803) or PowerShell fallback
        execSync(
            `powershell -Command "Expand-Archive -Path '${TEST_REPO_ZIP}' -DestinationPath '${destDir}'"`,
            { stdio: "pipe" },
        );
    } else {
        // macOS/Linux: use unzip
        execSync(`unzip -q "${TEST_REPO_ZIP}" -d "${destDir}"`, { stdio: "pipe" });
    }
}
