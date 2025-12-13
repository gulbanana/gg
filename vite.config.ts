import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// https://vitejs.dev/config/
export default defineConfig(async ({ command }) => ({
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
    target: ["es2022", "chrome97", "edge97", "safari15"],
    // Dev builds go to target/app (gitignored), production builds go to res/dist (committed)
    outDir: command === "serve" ? "target/app" : "res/dist",
  }
}));
