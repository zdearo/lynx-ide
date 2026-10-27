import { fileURLToPath, URL } from "node:url";
import process from "node:process";
import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  test: {
    // services/ has its own package, tests and tsconfig.
    exclude: [...configDefaults.exclude, "services/**"],
  },

  // CodeMirror loads on first use; listing it here stops the dev server from re-optimizing (and reloading the page) at that moment.
  optimizeDeps: {
    include: [
      "@codemirror/commands",
      "@codemirror/lang-css",
      "@codemirror/lang-html",
      "@codemirror/lang-javascript",
      "@codemirror/lang-json",
      "@codemirror/lang-markdown",
      "@codemirror/lang-php",
      "@codemirror/lang-python",
      "@codemirror/lang-rust",
      "@codemirror/language",
      "@codemirror/merge",
      "@codemirror/search",
      "@codemirror/state",
      "@codemirror/view",
      "@lezer/highlight",
      "@xyflow/react",
    ],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
