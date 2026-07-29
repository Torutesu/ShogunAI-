import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "../strings";
import { AnalyticsToggle } from "../AnalyticsToggle";

// Same detection App.tsx uses — the guide renders in a real browser tab during `pnpm dev:vite`
// (permission always "missing" there so the whole guide is visible), and talks to Rust only inside
// the Tauri window.
const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

type Appearance = "auto" | "light" | "dark";

const o = t.onboarding;

// Fire-and-forget a local funnel event (Rust logs it; nothing leaves the device). A no-op outside
// Tauri.
function track(name: string): void {
  if (IN_TAURI) void invoke("onboarding_event", { name }).catch(() => undefined);
}

function CheckIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--live)" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 12l5 5L20 6" />
    </svg>
  );
}

function ShieldIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--muted)" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 3l7.5 3.2v5.3c0 4.7-3.2 8.4-7.5 10.2-4.3-1.8-7.5-5.5-7.5-10.2V6.2L12 3z" />
    </svg>
  );
}

export function Onboarding() {
  // null = the first status read hasn't come back yet (avoids a flash of the guide when the app is
  // actually already trusted — a re-permission after an update).
  const [granted, setGranted] = useState<boolean | null>(IN_TAURI ? null : false);
  // True once the user has opened Settings at least once — reveals the "waiting" line and the
  // troubleshooting notes, which are noise before they've tried.
  const [opened, setOpened] = useState(false);
  const grantedLogged = useRef(false);

  // Theme: shared with the notch window through same-origin localStorage.
  useEffect(() => {
    const appearance = (localStorage.getItem("shogun.appearance") as Appearance | null) ?? "auto";
    document.documentElement.setAttribute("data-appearance", appearance);
  }, []);

  // Log that the guide was shown, read the initial status, and subscribe to the pushed
  // false→true edge. A slow poll backs up the event in case a push is missed.
  useEffect(() => {
    if (!IN_TAURI) return;
    track("guide_shown");
    void invoke<{ granted: boolean }>("onboarding_get")
      .then((s) => setGranted(s.granted))
      .catch(() => setGranted(false));

    const unlisten = listen<boolean>("accessibility-changed", (e) => setGranted(e.payload));
    const poll = window.setInterval(() => {
      void invoke<boolean>("accessibility_status")
        .then(setGranted)
        .catch(() => undefined);
    }, 1200);

    return () => {
      void unlisten.then((f) => f());
      window.clearInterval(poll);
    };
  }, []);

  // Log the grant exactly once, when it first flips on.
  useEffect(() => {
    if (granted && !grantedLogged.current) {
      grantedLogged.current = true;
      track("granted");
    }
  }, [granted]);

  const openSettings = useCallback(() => {
    setOpened(true);
    track("open_settings_clicked");
    if (IN_TAURI) void invoke("open_accessibility_settings").catch(() => undefined);
  }, []);

  const skip = useCallback(() => {
    track("skipped");
    if (IN_TAURI) void invoke("onboarding_finish", { action: "skipped" }).catch(() => undefined);
  }, []);

  const finish = useCallback(() => {
    track("open_shogun");
    if (IN_TAURI) void invoke("onboarding_finish", { action: "completed" }).catch(() => undefined);
  }, []);

  return (
    <div className="onb">
      <div className="onb-card glass">
        <header className="onb-head">
          <span className="onb-brand">
            <span className="onb-mark">⚔</span>
            {o.brand}
          </span>
          <span className={`onb-badge ${granted ? "ok" : ""}`}>
            <span className="onb-dot" />
            {granted === null ? o.checking : granted ? o.granted : o.notGranted}
          </span>
        </header>

        {granted ? <Success onFinish={finish} /> : <Guide opened={opened} onOpen={openSettings} onSkip={skip} />}
      </div>
    </div>
  );
}

function Guide({ opened, onOpen, onSkip }: { opened: boolean; onOpen: () => void; onSkip: () => void }) {
  return (
    <div className="onb-body">
      <div className="onb-tile">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 3l7.5 3.2v5.3c0 4.7-3.2 8.4-7.5 10.2-4.3-1.8-7.5-5.5-7.5-10.2V6.2L12 3z" />
          <path d="M9 11.8l2.2 2.2L15.2 10" />
        </svg>
      </div>

      <h1 className="onb-title">{o.guideTitle}</h1>
      <p className="onb-lead">{o.guideLead}</p>

      <div className="onb-cols">
        <div className="onb-col">
          <span className="onb-col-h">{o.doTitle}</span>
          {o.doItems.map((it) => (
            <span className="onb-item" key={it}>
              <CheckIcon />
              {it}
            </span>
          ))}
        </div>
        <div className="onb-col">
          <span className="onb-col-h">{o.wontTitle}</span>
          {o.wontItems.map((it) => (
            <span className="onb-item" key={it}>
              <ShieldIcon />
              {it}
            </span>
          ))}
        </div>
      </div>

      <div className="onb-steps">
        <span className="onb-col-h">{o.stepsTitle}</span>
        <ol>
          {o.steps.map((s, i) => (
            <li key={s}>
              <span className="onb-num">{i + 1}</span>
              {s}
            </li>
          ))}
        </ol>
      </div>

      {opened && (
        <div className="onb-trouble">
          <span className="onb-waiting">
            <span className="onb-spin" />
            {o.waiting}
          </span>
          <span className="onb-col-h">{o.troubleTitle}</span>
          <ul>
            {o.troubleItems.map((it) => (
              <li key={it}>{it}</li>
            ))}
          </ul>
        </div>
      )}

      <footer className="onb-foot">
        <div className="onb-skipwrap">
          <button className="onb-btn ghost" onClick={onSkip}>
            {o.skip}
          </button>
          <span className="onb-skipnote">{o.skipNote}</span>
        </div>
        <button className="onb-btn primary" onClick={onOpen}>
          {opened ? o.ctaAgain : o.cta}
        </button>
      </footer>
    </div>
  );
}

function Success({ onFinish }: { onFinish: () => void }) {
  return (
    <div className="onb-body onb-success">
      <div className="onb-tile ok">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="var(--live)" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M4 12l5 5L20 6" />
        </svg>
      </div>
      <h1 className="onb-title">{o.successTitle}</h1>
      <p className="onb-lead">{o.successLead}</p>
      <footer className="onb-foot">
        <AnalyticsToggle />
        <button className="onb-btn primary lg" onClick={onFinish}>
          {o.successCta}
        </button>
      </footer>
    </div>
  );
}
