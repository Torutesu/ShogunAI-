import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Desktop unit tests (#119). jsdom, no live Tauri: everything that would cross the IPC bridge is
// mocked at the module boundary in test/setup.ts, so a test that forgets to mock still cannot
// reach a real webview — the mock throws by default instead of silently succeeding.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "test/**/*.test.{ts,tsx}"],
  },
});
