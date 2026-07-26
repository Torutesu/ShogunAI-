import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { MeetingOverlay } from "./MeetingOverlay";
import "./styles.css";

// One bundle, two windows. The meeting overlay is its own small floating panel (Issue #7), so
// the entry point picks a root by the window it is running in rather than the notch panel
// having to host a surface that belongs next to the meeting.
function isMeetingWindow(): boolean {
  try {
    // Tauri exposes the label on the internals object; the URL fallback keeps `vite dev` in a
    // browser rendering the main app rather than nothing.
    const label = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } })
      .__TAURI_INTERNALS__?.metadata?.currentWindow?.label;
    return label === "meeting";
  } catch {
    return false;
  }
}

const root = document.getElementById("root");
if (root) {
  const meeting = isMeetingWindow();
  if (meeting) {
    document.documentElement.setAttribute("data-window", "meeting");
  }
  ReactDOM.createRoot(root).render(
    <React.StrictMode>{meeting ? <MeetingOverlay /> : <App />}</React.StrictMode>,
  );
}
