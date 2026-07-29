// Entry for the Full UI window (spec §D). Separate document from the notch panel: the panel is a
// native NSPanel hosting index.html, and the Full UI is an ordinary window, so they get their own
// bundles rather than a router inside one webview.
//
// The view is assembled in Rust (`full_ui_view`) and drawn here — the webview computes nothing
// (CLAUDE.md invariant 1). Outside Tauri, `pnpm dev:vite` still serves this page for design work,
// so it falls back to the sample fixture there and says so.

import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { FullUi } from "./fullui/FullUi";
import { SAMPLE_VIEW, SAMPLE_VIEW_STANDARD } from "./fullui/sample";
import type { FullUiView } from "./fullui/types";
import "./styles.css";

const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// The panel window is transparent because it floats over the desktop; this one is a real window,
// so give it a ground to sit on. Match the appearance the user chose in the panel's settings —
// same Tauri origin, so the `shogun.appearance` value the panel persists is shared here. Falls
// back to "auto" (follow the system) exactly like the panel's bootstrap; never hard-code dark.
function loadAppearance(): "auto" | "light" | "dark" {
  try {
    const v = JSON.parse(localStorage.getItem("shogun.appearance") ?? '"auto"');
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
}
document.documentElement.dataset.appearance = loadAppearance();
document.body.classList.add("full-window");

function Root(): JSX.Element {
  const [view, setView] = useState<FullUiView | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  // Esc closes the window. The traffic lights are the primary affordance, but this window floats
  // over your work like the panel does, and reaching for the mouse to dismiss it breaks the same
  // flow the notch exists to protect. ⌘W keeps working via the standard menu.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !IN_TAURI) return;
      void getCurrentWindow().close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!IN_TAURI) {
      // Design-time only: ?plan=standard previews the gated states. Never reached in the app.
      const standard = new URLSearchParams(location.search).get("plan") === "standard";
      setView(standard ? SAMPLE_VIEW_STANDARD : SAMPLE_VIEW);
      return;
    }
    invoke<FullUiView>("full_ui_view")
      .then(setView)
      .catch((e) => setFailed(String(e)));
  }, []);

  // Say what went wrong rather than silently falling back to fixture data — a window quietly
  // showing invented numbers is the one failure mode this screen must not have.
  if (failed) return <div className="full-boot">Couldn't read your context — {failed}</div>;
  if (!view) return <div className="full-boot" />;
  return <FullUi view={view} />;
}

const el = document.getElementById("root");
if (el) createRoot(el).render(<React.StrictMode><Root /></React.StrictMode>);
