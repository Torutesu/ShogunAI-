// Entry for the Full UI window (spec §D). Separate document from the notch panel: the panel is a
// native NSPanel hosting index.html, and the Full UI is an ordinary window, so they get their own
// bundles rather than a router inside one webview.
//
// While the Rust side doesn't serve the view yet, this renders placeholder data so the window can
// be built and reviewed. Swap the SAMPLE_VIEW branch for the invoke() result when the core lands.

import React from "react";
import { createRoot } from "react-dom/client";
import { FullUi } from "./fullui/FullUi";
import { SAMPLE_VIEW, SAMPLE_VIEW_STANDARD } from "./fullui/sample";
import "./styles.css";

// The panel window is transparent because it floats over the desktop; this one is a real window,
// so give it a ground to sit on.
document.documentElement.dataset.appearance = "dark";
document.body.classList.add("full-window");

// ?plan=standard renders the same day without the agent engine, so the gated states can be
// reviewed side by side. Dev affordance only — the real plan comes from the core.
const view = new URLSearchParams(location.search).get("plan") === "standard" ? SAMPLE_VIEW_STANDARD : SAMPLE_VIEW;

const el = document.getElementById("root");
if (el) createRoot(el).render(<React.StrictMode><FullUi view={view} /></React.StrictMode>);
