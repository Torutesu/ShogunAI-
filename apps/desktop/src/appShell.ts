export type Appearance = "auto" | "light" | "dark";

export type Citation = { event_id: number; source: string; title: string | null };

export type Msg = { role: "me" | "shogun"; text: string; citations?: Citation[] };

/** Hold-to-talk voice dialogue (#44). Rust owns lifecycle; notch expands on `voice_state`. */
export interface VoiceView {
  phase: "idle" | "recording" | "processing" | "response" | "error";
  transcript: string;
  response: string;
  error: string;
  level: number;
}

export interface Size {
  w: number;
  h: number;
}

/** Native measurements for the display currently hosting the notch panel. */
export interface PanelDisplayGeometry {
  is_notch: boolean;
  notch_height: number;
  handle_height: number;
}

/** CSS heights for a real hardware notch versus an external-display pseudo-notch. */
export function compactCssMetrics(geometry: PanelDisplayGeometry): {
  deadHeight: number;
  rowHeight: number;
  pillHeight: number;
} {
  if (!geometry.is_notch) {
    return {
      deadHeight: 0,
      rowHeight: geometry.handle_height,
      pillHeight: geometry.notch_height,
    };
  }
  return {
    deadHeight: geometry.notch_height,
    rowHeight: Math.max(0, geometry.handle_height - geometry.notch_height),
    pillHeight: geometry.notch_height,
  };
}

export type OpenPanelView = "chat" | "settings" | "hub";

/** Chat and Settings share one frame. Only Overview owns a separate remembered workspace size. */
export function panelSizeForView(view: OpenPanelView, chatSize: Size, hubSize: Size): Size {
  return view === "hub" ? hubSize : chatSize;
}

export function formatStorageBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value >= 10 || index === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
}
