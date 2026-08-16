import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { MeetingOverlay } from "./MeetingOverlay";
import { VoiceOverlay } from "./VoiceOverlay";
import { ScribeOverlay } from "./ScribeOverlay";
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

const root = document.getElementById("root");
if (root) {
  const label = currentLabel();
  const meeting = isMeetingLabel(label);
  const voice = label === "voice";
  const scribe = label === "scribe";
  // data-window uses the real label so CSS can target host vs each panel surface.
  document.documentElement.setAttribute(
    "data-window",
    meeting ? label : voice ? "voice" : scribe ? "scribe" : "main",
  );
  void import("@tauri-apps/api/core")
    .then(({ invoke }) =>
      invoke("ui_log", {
        msg: `window label=${label || "(unknown)"} → ${
          meeting ? `meeting overlay (${label})` : voice ? "voice overlay" : scribe ? "scribe overlay" : "main app"
        }`,
      }),
    )
    .catch(() => undefined);
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      {meeting ? <MeetingOverlay /> : voice ? <VoiceOverlay /> : scribe ? <ScribeOverlay /> : <App />}
    </React.StrictMode>,
  );
}
