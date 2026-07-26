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
      // Two documents, not two apps: the notch panel (index) and the Full UI window (spec §D).
      // They share styles.css and strings.ts, so the split is only at the entry.
      input: {
        index: resolve(__dirname, "index.html"),
        fullui: resolve(__dirname, "fullui.html"),
      },
    },
  },
});
