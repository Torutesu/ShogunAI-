// Shared test setup (#119): the Tauri IPC boundary is mocked HERE, once, so no test can reach a
// real webview — and a code path that fires an un-stubbed command fails loudly instead of
// resolving to undefined and letting the test pass on silence.
//
// Tests that care about specific commands override per-test:
//   const { invoke } = await import("@tauri-apps/api/core");
//   vi.mocked(invoke).mockResolvedValueOnce(payload);

import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Older modules still feature-detect Tauri through its former private global. New code uses the
// public `isTauri()` API mocked below. Keep both true so tests exercise IPC paths.
Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
Object.defineProperty(globalThis, "isTauri", { value: true, configurable: true });

// jsdom has no ResizeObserver; the scrubber uses one to track its viewport width. Defined as a
// plain global (not vi.stubGlobal) so a test calling vi.unstubAllGlobals() cannot remove it.
class NoopResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
Object.defineProperty(globalThis, "ResizeObserver", {
  value: NoopResizeObserver,
  configurable: true,
});

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: vi.fn(() => true),
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
    setFocus: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    onCloseRequested: vi.fn(async () => () => undefined),
  })),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: { getByLabel: vi.fn(async () => null) },
}));
