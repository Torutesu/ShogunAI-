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

// One context-action button, projected by the Rust `notch_actions` command from confidence-gated
// state (§6.1). `level` gates L1 (auto-eligible) vs L2/L3 (confirm) in the UI (invariant 4).
interface ActionView {
  label: string;
  level: "L1" | "L2" | "L3";
  rationale: string;
}

// Browser-preview mode: when the app is opened in a plain browser (`pnpm dev:vite` → localhost:1420)
// there is no Tauri runtime, so `invoke`/`listen` would reject. In that case we render the panel in
// its Expanded state with representative mock data — so the UI can be seen and iterated without the
// (fragile, macOS-only) NSPanel. On device, `IN_TAURI` is true and nothing here runs.
const IN_TAURI = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

const MOCK_CTX: ContextPayload = {
  bundle_id: "com.apple.mail",
  title_masked: "Inbox — Q3 roadmap",
  text: "Thanks — I'll send the final deck by Friday. Still waiting on legal to sign off before we share it externally.",
  captured_at_ms: 0,
  partial: false,
};

// Representative of a real `notch_actions` result: state-derived actions (L1/L2) plus the FR-CF-04
// generic fallbacks, so the panel shows the level tags and the four-button cap.
const MOCK_ACTIONS: ActionView[] = [
  { label: "Draft reply", level: "L1", rationale: "reply needed — Q3 roadmap thread" },
  { label: "Nudge: legal sign-off", level: "L2", rationale: "waiting on legal to reply" },
  { label: "Search memory", level: "L1", rationale: "Search memory" },
  { label: "Save a note", level: "L1", rationale: "Save a note" },
];

export function App(): JSX.Element {
  const [uiState, setUiState] = useState<UiState>(IN_TAURI ? "idle" : "expanded");
  const [ctx, setCtx] = useState<ContextPayload | null>(IN_TAURI ? null : MOCK_CTX);
  const [actions, setActions] = useState<ActionView[]>(IN_TAURI ? [] : MOCK_ACTIONS);
  const [pendingConfirm, setPendingConfirm] = useState<{ index: number; id: number } | null>(null);
  const stateRef = useRef<UiState>(IN_TAURI ? "idle" : "expanded");

  useEffect(() => {
    // In browser-preview there is no Tauri runtime — skip all IPC and keep the mocked panel.
    if (!IN_TAURI) return;
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

  // On expand, pull the real context actions for the focused screen (§6.1). Best-effort: if the
  // command fails or returns none, the panel falls back to the placeholder labels.
  useEffect(() => {
    if (!IN_TAURI) return;
    if (uiState !== "expanded") return;
    let live = true;
    void invoke<ActionView[]>("notch_actions")
      .then((a) => {
        if (live) setActions(a);
      })
      .catch(() => {
        /* command unavailable (e.g. no DB) — keep placeholders */
      });
    return () => {
      live = false;
    };
  }, [uiState]);

  // Run a context action (§6.6.2): L1 executes immediately; L2 returns "confirm:<id>" and the
  // button turns into a one-tap confirm (a second tap on the same button confirms).
  const runAction = (index: number): void => {
    // Browser-preview: no engine — just demonstrate the L2 confirm toggle locally.
    if (!IN_TAURI) {
      const a = actions[index];
      if (a && a.level !== "L1" && pendingConfirm?.index !== index) {
        setPendingConfirm({ index, id: -1 });
      } else {
        setPendingConfirm(null);
      }
      return;
    }
    if (pendingConfirm?.index === index) {
      void invoke<string>("confirm_notch_action", { id: pendingConfirm.id }).finally(() =>
        setPendingConfirm(null),
      );
      return;
    }
    void invoke<string>("run_notch_action", { index }).then((res) => {
      if (res.startsWith("confirm:")) {
        setPendingConfirm({ index, id: Number(res.slice("confirm:".length)) });
      } else {
        setPendingConfirm(null);
      }
    });
  };

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
            {actions.length > 0
              ? actions.map((a, i) => (
                  <button
                    key={i}
                    type="button"
                    className={`notch__action notch__action--${a.level.toLowerCase()}`}
                    title={a.rationale}
                    onClick={() => void runAction(i)}
                  >
                    <span className="notch__action-level">{a.level}</span>
                    <span className="notch__action-label">{a.label}</span>
                    {pendingConfirm?.index === i ? (
                      <span className="notch__action-confirm">tap to confirm</span>
                    ) : null}
                  </button>
                ))
              : [t.action1, t.action2, t.action3, t.action4].map((label, i) => (
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
