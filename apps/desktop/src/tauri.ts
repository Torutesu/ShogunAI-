// The one place that answers "am I running inside the app, or in a browser?".
//
// It used to be a const in App.tsx, which meant every module that needed it either imported App
// (a cycle) or re-derived it. Both the panel and the onboarding flow ask, so it lives here.

import { invoke } from "@tauri-apps/api/core";

export const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

/// Report a webview-side failure to the terminal via Rust — a silent catch made real errors
/// (a missing window-API permission) look like "the button does nothing".
export function uiLog(msg: string): void {
  if (IN_TAURI) void invoke("ui_log", { msg }).catch(() => undefined);
}

/// Invoke a command, or resolve to `fallback` when there is no backend (browser preview, or a
/// command this build does not have). Keeps optional-feature call sites from each growing their
/// own try/catch.
export function ask<T>(cmd: string, args: Record<string, unknown>, fallback: T): Promise<T> {
  if (!IN_TAURI) return Promise.resolve(fallback);
  return invoke<T>(cmd, args).catch(() => fallback);
}
