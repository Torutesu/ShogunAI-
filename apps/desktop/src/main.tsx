import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { Settings } from "./Settings";
import "./styles.css";

// Both windows load this same bundle; pick what to render by the Tauri window label. The `settings`
// window shows the connections screen; everything else is the notch panel. Guarded so a plain
// browser (vite dev without Tauri) still renders the panel instead of throwing.
function isSettingsWindow(): boolean {
  try {
    return getCurrentWindow().label === "settings";
  } catch {
    return false;
  }
}

const root = document.getElementById("root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>{isSettingsWindow() ? <Settings /> : <App />}</React.StrictMode>,
  );
}
