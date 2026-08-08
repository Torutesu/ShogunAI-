import { invoke } from "@tauri-apps/api/core";

const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

/** Forward webview failures to the Rust dev log (terminal). */
export function uiLog(msg: string): void {
  if (IN_TAURI) void invoke("ui_log", { msg }).catch(() => undefined);
}
