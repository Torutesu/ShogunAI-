// Entry point for the browser preview (`preview.html`). Not part of the shipped app: the Tauri
// window loads `index.html` → `src/main.tsx`, which never touches this directory.
//
// Import order matters. `./bridge` installs the mock `window.__TAURI_INTERNALS__` as a module side
// effect, and `App.tsx` reads that object at module scope to decide whether it is running inside
// Tauri — so the bridge must be evaluated before `./Stage` (which imports App) is.
import "./bridge";

import ReactDOM from "react-dom/client";
import { Stage } from "./Stage";
import "../styles.css";
import "./preview.css";

const root = document.getElementById("root");
if (root) {
  // No StrictMode here: its double-invoked effects would fire the App's boot IPC twice and make
  // the preview's IPC log misleading. The shipped entry keeps StrictMode.
  ReactDOM.createRoot(root).render(<Stage />);
}
