import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

document.documentElement.dataset.appearance = (() => {
  try {
    const v = JSON.parse(localStorage.getItem("shogun.appearance") ?? '"auto"');
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
})();

const el = document.getElementById("root");
if (el) createRoot(el).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
