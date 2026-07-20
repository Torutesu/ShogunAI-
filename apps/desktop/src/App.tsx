import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./strings";

// Mirror of the closed IPC contract (spec §3.11.2 / §6.1.1). The webview only: class-swaps on
// `state`, reports paint-done (rAF×2 → `painted`), and forwards input (`promote` / `interact` /
// `open_full_ui` / `collapse_request` / `anim_done` / `clock_sync_ack`). No timers, no state
// machine, no cache here (data centre of gravity is Rust). Two open levels per FR-NU-01:
// Hover = preview (1 action + status line), Expanded = full panel (≤4 actions + Full UI).

type UiState = "idle" | "hoverintent" | "hover" | "expanded" | "collapsing" | "hidden";

interface StatePayload {
  state: UiState;
  t0_mono_ns: number;
}

interface ContextPayload {
  bundle_id: string;
  title_masked: string;
  text: string;
  captured_at_ms: number;
  partial: boolean;
}

interface ClockSyncPayload {
  seq: number;
  rust_mono_ns: number;
}

/**
 * Notify Rust that the preview frame has actually been presented. Two nested rAFs approximate
 * "after the next frame is composited" — this is the `t1` of the preview-open latency (the
 * Phase 0 Q2 measurement, spec §4.2.1). The rAF-vs-composite gap is calibrated on-device.
 */
function notifyPainted(state: UiState): void {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void invoke("painted", { state, t1PerfMs: performance.now() });
    });
  });
}

function ContextLine({ ctx }: { ctx: ContextPayload | null }): JSX.Element {
  if (!ctx) return <span className="notch__preview--empty">{t.noContext}</span>;
  const snippet = ctx.text ? `${ctx.text.length} ${t.charsCaptured} · ${ctx.text.replace(/\s+/g, " ").slice(0, 80)}` : t.noText;
  return (
    <span className="notch__ctx">
      <span className="notch__ctx-app">
        {ctx.bundle_id || ctx.title_masked || "—"}
        {ctx.partial ? t.partialSuffix : ""}
      </span>
      <span className="notch__ctx-text">{snippet}</span>
    </span>
  );
}

export function App(): JSX.Element {
  const [uiState, setUiState] = useState<UiState>("idle");
  const [ctx, setCtx] = useState<ContextPayload | null>(null);
  const stateRef = useRef<UiState>("idle");

  useEffect(() => {
    // Webview-alive ping: proves the frontend booted and invoke reaches Rust.
    void invoke("interact", { kind: "boot" });
    const unlisteners: Array<Promise<() => void>> = [];

    unlisteners.push(
      listen<StatePayload>("state", (e) => {
        const next = e.payload.state;
        stateRef.current = next;
        setUiState(next);
        // The preview (Hover) is the visible open the Phase 0 latency measures.
        if (next === "hover") notifyPainted(next);
      }),
    );

    unlisteners.push(
      listen<ContextPayload>("context", (e) => {
        setCtx(e.payload);
      }),
    );

    // Clock-sync round trip (spec §4.1): answer each ping with performance.now() so Rust
    // can estimate the JS↔Rust offset from the minimum-RTT sample.
    unlisteners.push(
      listen<ClockSyncPayload>("clock_sync", (e) => {
        void invoke("clock_sync_ack", { seq: e.payload.seq, jsPerfMs: performance.now() });
      }),
    );

    // Esc collapses an open preview/panel (delivered only while the panel is key).
    const onKeyDown = (e: KeyboardEvent): void => {
      const s = stateRef.current;
      if (e.key === "Escape" && (s === "hover" || s === "expanded")) {
        void invoke("collapse_request", { reason: "esc" });
      }
    };
    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      unlisteners.forEach((p) => void p.then((off) => off()));
    };
  }, []);

  const openState = uiState === "hover" || uiState === "expanded";

  return (
    <div
      className={`notch notch--${uiState}`}
      onClick={(e) => {
        // A click on the transparent margin (outside the visible panel) collapses it.
        if (e.target === e.currentTarget && openState) {
          void invoke("collapse_request", { reason: "outside_click" });
        }
      }}
    >
      <div className="notch__idle-shell" />
      <div
        className="notch__panel"
        onTransitionEnd={(e) => {
          // Report collapse-animation completion (transform is the driver; the property
          // filter keeps the two animated properties from double-firing).
          if (e.propertyName === "transform" && stateRef.current === "collapsing") {
            void invoke("anim_done", { state: "collapsing" });
          }
        }}
      >
        {/* Preview level (Hover): one action + one context line. Clicking promotes to full. */}
        <button className="notch__preview" type="button" onClick={() => void invoke("promote")}>
          <span className="notch__preview-action">{t.action1}</span>
          <ContextLine ctx={ctx} />
        </button>

        {/* Full level (Expanded): up to four actions, context, and the Full UI entry point. */}
        <div className="notch__full">
          <div className="notch__actions">
            {[t.action1, t.action2, t.action3, t.action4].map((label, i) => (
              <button key={i} type="button" onClick={() => void invoke("interact", { kind: "click" })}>
                {label}
              </button>
            ))}
          </div>
          <div className="notch__full-context">
            <ContextLine ctx={ctx} />
          </div>
          <button className="notch__fullui" type="button" onClick={() => void invoke("open_full_ui")}>
            {t.openFullUi}
          </button>
        </div>
      </div>
    </div>
  );
}
