// The launch moment: the mark folds itself together while the core comes up.
//
// Deliberately the whole document — no copy, no spinner, no version string. A splash that says
// something is a splash the reader has to finish reading before the app is allowed to open.
//
// Nothing here talks to Rust. The window is built hidden, shown when this page finishes loading,
// and closed on a timer the Rust side owns (splash.rs) — so a webview that never loads means no
// splash at all rather than a window stuck over the desktop.
import React from "react";
import ReactDOM from "react-dom/client";
import { AnimatedLogo } from "./Logo";
import "./styles.css";
import "./styles/splash.css";

const root = document.getElementById("root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <div className="splash">
        <AnimatedLogo size={112} motion="unfold" />
      </div>
    </React.StrictMode>,
  );
}
