import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

// Tauri v2 dev server config. Port is fixed so tauri.conf.json devUrl matches.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    // Spike targets recent Apple Silicon WebKit only (spec: macOS 14+).
    target: "safari16",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      // Separate documents, not separate apps: the notch panel (index), the Full UI window
      // (spec §D), and the first-run Accessibility guide (onboarding.html, Issue #46). They share
      // styles.css and strings.ts, so the split is only at the entry.
      input: {
        index: resolve(__dirname, "index.html"),
        fullui: resolve(__dirname, "fullui.html"),
        onboarding: resolve(__dirname, "onboarding.html"),
        "visual-recall": resolve(__dirname, "visual-recall.html"),
      },
    },
  },
});
