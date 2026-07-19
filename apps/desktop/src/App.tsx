import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./strings";

// Mirror of the closed IPC contract (spec §3.11.2). The webview does exactly three things:
// class-swap on `state`, paint-done notification (rAF×2 → `painted`), and input forwarding
// (`interact` / `collapse_request` / `anim_done` / `clock_sync_ack`). No timers, no state
// machine, no cache here (data centre of gravity is Rust).

type UiState = "idle" | "hoverintent" | "expanded" | "collapsing";

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
 * Notify Rust that the Expanded frame has actually been presented. Two nested rAFs
 * approximate "after the next frame is composited" — this is the `t1` of the Q2 expand
 * latency measurement (spec §4.2.1). The rAF-vs-composite gap is calibrated on-device.
 */
function notifyPainted(state: UiState): void {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void invoke("painted", { state, t1PerfMs: performance.now() });
    });
  });
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
        if (next === "expanded") notifyPainted(next);
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

    // Esc → T4b collapse (delivered only while the panel is key; harmless otherwise).
    const onKeyDown = (e: KeyboardEvent): void => {
      if (e.key === "Escape" && stateRef.current === "expanded") {
        void invoke("collapse_request", { reason: "esc" });
      }
    };
    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      unlisteners.forEach((p) => void p.then((off) => off()));
    };
  }, []);

  // Dummy Expanded content: three static action buttons + a context preview fed by the
  // real AX cache (spec §2.1 item 6). Buttons forward interactions for the Q4 tally.
  // A click on the transparent margin (outside the visible panel) is T4c.
  return (
    <div
      className={`notch notch--${uiState}`}
      onClick={(e) => {
        if (e.target === e.currentTarget && stateRef.current === "expanded") {
          void invoke("collapse_request", { reason: "outside_click" });
        }
      }}
    >
      <div className="notch__idle-shell" />
      <div
        className="notch__expanded"
        onTransitionEnd={(e) => {
          // T6: report collapse-animation completion (property filter keeps the two
          // transition properties from double-firing — transform is the driver).
          if (e.propertyName === "transform" && stateRef.current === "collapsing") {
            void invoke("anim_done", { state: "collapsing" });
          }
        }}
      >
        <div className="notch__actions">
          <button type="button" onClick={() => void invoke("interact", { kind: "click" })}>
            {t.action1}
          </button>
          <button type="button" onClick={() => void invoke("interact", { kind: "click" })}>
            {t.action2}
          </button>
          <button type="button" onClick={() => void invoke("interact", { kind: "click" })}>
            {t.action3}
          </button>
        </div>
        <div className="notch__preview">
          {ctx ? (
            <span>
              {ctx.bundle_id}
              {ctx.partial ? t.partialSuffix : ""}
            </span>
          ) : (
            <span className="notch__preview--empty">{t.noContext}</span>
          )}
        </div>
      </div>
    </div>
  );
}
