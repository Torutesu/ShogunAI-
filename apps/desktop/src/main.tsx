import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { MeetingOverlay } from "./MeetingOverlay";
import { VoiceOverlay } from "./VoiceOverlay";
import "./styles.css";

// One bundle, many windows. Meeting host + independent opaque panel windows each mount
// MeetingOverlay; the entry point picks a root by window label.
function currentLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "";
  }
}

function isMeetingLabel(label: string): boolean {
  return (
    label === "meeting" ||
    label === "meeting-cc" ||
    label === "meeting-canvas" ||
    label === "meeting-chat"
  );
}

/** The appearance the user picked, persisted by the notch panel. Same Tauri origin, so the
 *  overlays read the value the panel wrote. Without this the overlay windows fell through to the
 *  bare `:root` bootstrap — i.e. permanently dark, while the rest of the app followed Light. */
function loadAppearance(): "auto" | "light" | "dark" {
  try {
    const v = JSON.parse(localStorage.getItem("shogun.appearance") ?? '"auto"');
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
}

const root = document.getElementById("root");
if (root) {
  const label = currentLabel();
  const meeting = isMeetingLabel(label);
  const voice = label === "voice";
  // data-window uses the real label so CSS can target host vs each panel surface.
  document.documentElement.setAttribute(
    "data-window",
    meeting ? label : voice ? "voice" : "main",
  );
  // App sets this for the notch panel itself; the overlays get their own windows and must set it
  // too, or they render dark on a Light setup.
  if (meeting || voice) document.documentElement.dataset.appearance = loadAppearance();
  void import("@tauri-apps/api/core")
    .then(({ invoke }) =>
      invoke("ui_log", {
        msg: `window label=${label || "(unknown)"} → ${
          meeting ? `meeting overlay (${label})` : voice ? "voice overlay" : "main app"
        }`,
      }),
    )
    .catch(() => undefined);
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      {meeting ? <MeetingOverlay /> : voice ? <VoiceOverlay /> : <App />}
    </React.StrictMode>,
  );
}
