import React from "react";
import ReactDOM from "react-dom/client";
import { Onboarding } from "./onboarding/Onboarding";
// styles.css carries the design tokens (:root / [data-appearance]); onboarding.css layers the
// guide-specific rules on top. Same import shape as main.tsx.
import "./styles.css";
import "./onboarding/onboarding.css";

const root = document.getElementById("root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <Onboarding />
    </React.StrictMode>,
  );
}
