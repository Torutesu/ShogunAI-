import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

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
  },
});
