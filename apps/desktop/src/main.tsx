import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { MeetingOverlay } from "./MeetingOverlay";
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

const root = document.getElementById("root");
if (root) {
  const label = currentLabel();
  const meeting = label === "meeting";
  document.documentElement.setAttribute("data-window", meeting ? "meeting" : "main");
  // Say which root was chosen. A blank overlay and a missing overlay look identical on screen;
  // in the log they do not.
  // Reported to Rust, not just the webview console: a blank overlay and a missing overlay look
  // identical on screen, and the webview's console is not in the dev log.
  void import("@tauri-apps/api/core")
    .then(({ invoke }) =>
      invoke("ui_log", {
        msg: `window label=${label || "(unknown)"} → ${meeting ? "meeting overlay" : "main app"}`,
      }),
    )
    .catch(() => undefined);
  ReactDOM.createRoot(root).render(
    <React.StrictMode>{meeting ? <MeetingOverlay /> : <App />}</React.StrictMode>,
  );
}
