import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

// Mirror of the closed IPC contract (spec §3.11.2). The webview does exactly three things:
// class-swap on `state`, paint-done notification (rAF×2 → `painted`), and input forwarding
// (`interact`). No timers, no state machine, no cache here (data centre of gravity is Rust).

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

    return () => {
      unlisteners.forEach((p) => void p.then((off) => off()));
    };
  }, []);

  // Dummy Expanded content: three static action buttons + a context preview fed by the
  // real AX cache (spec §2.1 item 6). Buttons forward interactions for the Q4 tally.
  return (
    <div className={`notch notch--${uiState}`}>
      <div className="notch__idle-shell" />
      <div className="notch__expanded">
        <div className="notch__actions">
          <button type="button" onClick={() => void invoke("interact", { kind: "click" })}>
            Action 1
          </button>
          <button type="button" onClick={() => void invoke("interact", { kind: "click" })}>
            Action 2
          </button>
          <button type="button" onClick={() => void invoke("interact", { kind: "click" })}>
            Action 3
          </button>
        </div>
        <div className="notch__preview">
          {ctx ? (
            <span>
              {ctx.bundle_id}
              {ctx.partial ? " (partial)" : ""}
            </span>
          ) : (
            <span className="notch__preview--empty">no context</span>
          )}
        </div>
      </div>
    </div>
  );
}
