import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri v2 dev server config. Port is fixed so tauri.conf.json devUrl matches.
//
// Two HTML entries live here:
//   index.html   — the app the Tauri window loads. This is the ONLY entry in the shipped build.
//   preview.html — the browser preview (src/preview/*), a mock-IPC harness for design work.
// The preview is a separate entry rather than a flag inside the app, so the mock data can never
// reach the app bundle: `SHOGUN_PREVIEW=1` builds the preview instead, as one self-contained
// file (see scripts/build-preview.mjs). In dev both are served — open /preview.html.
const preview = process.env.SHOGUN_PREVIEW === "1";

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
    outDir: preview ? "dist-preview" : "dist",
    emptyOutDir: true,
    // The preview is shared as a single file, so its assets are inlined rather than emitted.
    assetsInlineLimit: preview ? 100_000_000 : 4096,
    rollupOptions: {
      input: preview ? "preview.html" : "index.html",
      output: preview ? { inlineDynamicImports: true } : {},
    },
  },
});
