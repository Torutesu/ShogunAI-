// Shared test setup (#119): the Tauri IPC boundary is mocked HERE, once, so no test can reach a
// real webview — and a code path that fires an un-stubbed command fails loudly instead of
// resolving to undefined and letting the test pass on silence.
//
// Tests that care about specific commands override per-test:
//   const { invoke } = await import("@tauri-apps/api/core");
//   vi.mocked(invoke).mockResolvedValueOnce(payload);

import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    throw new Error(`unmocked Tauri invoke in test: ${cmd}`);
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
  emit: vi.fn(async () => undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    label: "test",
    setSize: vi.fn(async () => undefined),
    setPosition: vi.fn(async () => undefined),
    hide: vi.fn(async () => undefined),
    show: vi.fn(async () => undefined),
  })),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: { getByLabel: vi.fn(async () => null) },
}));
