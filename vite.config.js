import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri sets this env var when running `tauri android dev` / `tauri ios dev`
// so the Vite dev server binds to an address the device can actually reach.
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],

  // Prevent Vite from obscuring Rust errors in the terminal.
  clearScreen: false,

  server: {
    // Bind to the LAN/USB host on mobile, otherwise localhost.
    host: host || "localhost",
    port: 1420,
    strictPort: true,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Don't watch the Rust side; Tauri handles rebuilding that.
      ignored: ["**/src-tauri/**"],
    },
  },

  // Produce output Tauri picks up via `frontendDist: "../dist"`.
  build: {
    target: "es2021",
    minify: process.env.TAURI_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
