import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { playwright } from "@vitest/browser-playwright";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [svelte()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 6973,
    strictPort: true,
    watch: {
      // 3. tell vite to ignore watching rust directories
      ignored: ["**/src/**", "**/target/**"],
    },
  },

  build: {
    target: ["es2022", "chrome97", "edge97", "safari15"]
  },

  test: {
    projects: [
      {
        extends: true,
        test: {
          name: "app",
          include: ["app/**/*.test.ts"],
          browser: {
            provider: playwright(),
            enabled: true,
            headless: true,
            instances: [{ browser: "chromium" }],
          },
        },
      },
      {
        test: {
          name: "e2e",
          include: ["e2e/**/*.test.ts"],
          testTimeout: 120000,
          hookTimeout: 120000,
          fileParallelism: false,
        },
      },
    ],
  },
}));
