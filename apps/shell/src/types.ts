import { copy } from "./strings";

/** Mirrors `apps/shell/src-tauri/src/view.rs`. The webview does not derive these fields. */

export interface EmptyPane {
  body: string;
}

export type PaneId =
  | "today"
  | "health"
  | "sources"
  | "memory"
  | "activity"
  | "trace"
  | "settings";

export interface ShellView {
  os: "windows" | "linux" | "other";
  app_data_dir: string;
  secrets_backend: string;
  secrets_ready: boolean;
  secrets_detail: string;
  close_behavior: string;
  today: EmptyPane;
  health: EmptyPane;
  sources: EmptyPane;
  memory: EmptyPane;
  activity: EmptyPane;
  trace: EmptyPane;
}

export const PANE_CHROME: Record<Exclude<PaneId, "settings">, { title: string; sub: string }> = {
  today: { title: copy.navToday, sub: copy.todaySub },
  health: { title: copy.navHealth, sub: copy.healthSub },
  sources: { title: copy.navSources, sub: copy.sourcesSub },
  memory: { title: copy.navMemory, sub: copy.memorySub },
  activity: { title: copy.navActivity, sub: copy.activitySub },
  trace: { title: copy.navTrace, sub: copy.traceSub },
};
