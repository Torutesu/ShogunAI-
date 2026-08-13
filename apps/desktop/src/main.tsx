import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { MeetingOverlay } from "./MeetingOverlay";
import { VoiceOverlay } from "./VoiceOverlay";
import "./styles.css";

// One bundle, two windows. The meeting overlay is its own small floating panel (Issue #7), so
// the entry point picks a root by the window it is running in rather than the notch panel having
// to host a surface that belongs next to the meeting.
//
// Asked through the official API rather than by reading Tauri's internals: the internals are an
// implementation detail, and when the shape changed the check silently answered "not the meeting
// window" — which renders nothing into a transparent window, i.e. a panel that exists, is shown,
// and cannot be seen.
function currentLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "";
  }
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
  const meeting = label === "meeting";
  const voice = label === "voice";
  document.documentElement.setAttribute("data-window", meeting ? "meeting" : voice ? "voice" : "main");
  // App sets this for the notch panel itself; the overlays get their own windows and must set it
  // too, or they render dark on a Light setup.
  if (meeting || voice) document.documentElement.dataset.appearance = loadAppearance();
  // Say which root was chosen. A blank overlay and a missing overlay look identical on screen;
  // in the log they do not.
  // Reported to Rust, not just the webview console: a blank overlay and a missing overlay look
  // identical on screen, and the webview's console is not in the dev log.
  void import("@tauri-apps/api/core")
    .then(({ invoke }) =>
      invoke("ui_log", {
        msg: `window label=${label || "(unknown)"} → ${meeting ? "meeting overlay" : voice ? "voice overlay" : "main app"}`,
      }),
    )
    .catch(() => undefined);
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      {meeting ? <MeetingOverlay /> : voice ? <VoiceOverlay /> : <App />}
    </React.StrictMode>,
  );
}
