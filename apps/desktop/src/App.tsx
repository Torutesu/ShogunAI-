import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./strings";

// SHOGUN notch panel. The panel hangs from the notch, lives on every space in the background,
// and is driven by the Rust engine's closed IPC contract (spec §3.11.2): the webview class-swaps
// on `state`, shows live `context`, answers `clock_sync`, reports paint (`painted`), and forwards
// input (`promote` / `interact` / `collapse_request` / `anim_done`). No timers, no state machine,
// no data layer here — the centre of gravity is Rust (CLAUDE.md invariant 1).
//
// State flow (mirrors NotchEngine): idle → hoverintent → hover (peek) → expanded (full panel) →
// collapsing → idle. In a plain browser (no Tauri) it pins to `expanded` on mock data so the
// design iterates without a Mac.

type UiState = "idle" | "hoverintent" | "hover" | "expanded" | "collapsing";
type Appearance = "auto" | "light" | "dark";
type Msg = { role: "me" | "shogun"; text: string };

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
interface Status {
  app: string;
  commitments: number;
  open_loops: number;
  has_key: boolean;
}
interface StateItem {
  text: string;
  meta: string;
}
interface StateView {
  commitments: StateItem[];
  open_loops: StateItem[];
}

const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

const MOCK_STATUS: Status = { app: "com.apple.mail", commitments: 2, open_loops: 1, has_key: false };
const MOCK_STATE: StateView = {
  commitments: [
    { text: "Send Alice the Q3 deck", meta: "overdue" },
    { text: "Reply to the vendor about pricing", meta: "70% sure" },
  ],
  open_loops: [{ text: "Waiting on legal sign-off", meta: "3d waiting" }],
};

function appName(bundle: string): string {
  if (!bundle) return t.yourScreen;
  const seg = bundle.split(".").pop() || bundle;
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}

/** rAF×2 ≈ "presented after the next composite" — the `t1` of the preview-open latency (spec §4.2.1). */
function notifyPainted(state: UiState): void {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void invoke("painted", { state, t1PerfMs: performance.now() });
    });
  });
}

export function App(): JSX.Element {
  const [ui, setUi] = useState<UiState>(IN_TAURI ? "idle" : "expanded");
  const [status, setStatus] = useState<Status | null>(IN_TAURI ? null : MOCK_STATUS);
  const [state, setState] = useState<StateView>(IN_TAURI ? { commitments: [], open_loops: [] } : MOCK_STATE);
  const [ctxApp, setCtxApp] = useState<string>("");
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  const [appearance, setAppearance] = useState<Appearance>("auto");
  const [pinned, setPinned] = useState(false);
  const [showState, setShowState] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const uiRef = useRef<UiState>(ui);
  const pinnedRef = useRef(pinned);
  const threadRef = useRef<HTMLDivElement>(null);
  uiRef.current = ui;
  pinnedRef.current = pinned;

  // Appearance → <html data-appearance>. "dark" is the CSS default (no attribute needed).
  useEffect(() => {
    const el = document.documentElement;
    if (appearance === "dark") el.removeAttribute("data-appearance");
    else el.setAttribute("data-appearance", appearance);
  }, [appearance]);

  // Closed IPC contract: state / context / clock_sync, plus the boot ping.
  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke("interact", { kind: "boot" });
    const offs: Array<Promise<() => void>> = [];

    offs.push(
      listen<StatePayload>("state", (e) => {
        const next = e.payload.state;
        setUi(next);
        if (next === "idle") setShowSettings(false);
        if (next === "hover") notifyPainted(next); // the Phase-0 preview-open latency sample
      }),
    );
    offs.push(listen<ContextPayload>("context", (e) => setCtxApp(e.payload.bundle_id || e.payload.title_masked || "")));
    offs.push(
      listen<ClockSyncPayload>("clock_sync", (e) =>
        void invoke("clock_sync_ack", { seq: e.payload.seq, jsPerfMs: performance.now() }),
      ),
    );
    return () => offs.forEach((p) => void p.then((off) => off()));
  }, []);

  // Live status / state tables (read-only pull; the data lives in Rust).
  useEffect(() => {
    if (!IN_TAURI) return;
    let live = true;
    const pull = (): void => {
      void invoke<Status>("shogun_status").then((s) => live && setStatus(s)).catch(() => undefined);
      void invoke<StateView>("shogun_state").then((s) => live && s && setState(s)).catch(() => undefined);
    };
    pull();
    const id = setInterval(pull, 3000);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, []);

  // Pin = defeat the engine's idle-collapse by resetting its timer on a heartbeat. Unpinning
  // stops the heartbeat and the panel auto-hides on the next idle timeout (or hover-out).
  useEffect(() => {
    if (!IN_TAURI || !pinned) return;
    const id = setInterval(() => void invoke("interact", { kind: "pin" }).catch(() => undefined), 8000);
    return () => clearInterval(id);
  }, [pinned]);

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
  }, [msgs, thinking, ui]);

  const nudge = useCallback((): void => {
    // Any real interaction resets the Expanded idle timer (ignored in other states).
    if (IN_TAURI) void invoke("interact", { kind: "click" }).catch(() => undefined);
  }, []);

  const send = useCallback((): void => {
    const q = input.trim();
    if (!q || thinking) return;
    setInput("");
    setMsgs((m) => [...m, { role: "me", text: q }]);
    setThinking(true);
    nudge();
    const finish = (text: string): void => {
      setMsgs((m) => [...m, { role: "shogun", text }]);
      setThinking(false);
    };
    if (!IN_TAURI) {
      setTimeout(() => finish("I'd start with the overdue deck for Alice — want me to draft it at your cursor?"), 700);
      return;
    }
    void invoke<string>("shogun_chat", { message: q })
      .then((r) => finish(r || t.noAnswer))
      .catch((e) => finish(`${t.answerFailed}: ${e}`));
  }, [input, thinking, nudge]);

  const draftAtCursor = useCallback((): void => {
    nudge();
    if (IN_TAURI) void invoke("inline_at_cursor").catch(() => undefined);
  }, [nudge]);

  const requestCollapse = (reason: "esc" | "outside_click"): void => {
    if (pinnedRef.current) return; // pinned panels ignore look-away collapses
    if (IN_TAURI) void invoke("collapse_request", { reason }).catch(() => undefined);
    else setUi("idle");
  };

  const open = ui === "hover" || ui === "expanded";
  const totalState = state.commitments.length + state.open_loops.length;
  const live = appName(ctxApp || status?.app || "");

  return (
    <div
      className={`notch notch--${ui}${pinned ? " is-pinned" : ""}`}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) requestCollapse("esc");
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget && open) requestCollapse("outside_click");
      }}
    >
      {/* idle affordance — a slim tongue under the notch you aim at to reveal the panel */}
      <div className="dock" aria-hidden={open} />

      <div
        className="sheet"
        onTransitionEnd={(e) => {
          if (e.propertyName === "transform" && uiRef.current === "collapsing") {
            void invoke("anim_done", { state: "collapsing" }).catch(() => undefined);
          }
        }}
      >
        {/* PEEK (hover): a compact card that promotes to the full panel on click */}
        <button className="peek" type="button" onClick={() => (IN_TAURI ? void invoke("promote").catch(() => undefined) : setUi("expanded"))}>
          <span className="peek__live">
            <span className="live__dot" />
            {t.reading} <b>{live}</b>
          </span>
          <span className="peek__right">
            {totalState > 0 ? (
              <span className="peek__count">
                {state.commitments.length} {t.due} · {state.open_loops.length} {t.waiting}
              </span>
            ) : (
              <span className="peek__hint">{t.peekHint}</span>
            )}
          </span>
        </button>

        {/* FULL (expanded): the working panel */}
        <div className="panel">
          {showSettings ? (
            <Settings
              appearance={appearance}
              setAppearance={setAppearance}
              pinned={pinned}
              setPinned={(v) => {
                setPinned(v);
                nudge();
              }}
              hasKey={!!status?.has_key}
              onDone={() => setShowSettings(false)}
            />
          ) : (
            <>
              <header className="head">
                <span className="live">
                  <span className="live__dot" />
                  {t.reading} <b>{live}</b>
                </span>
                <div className="head__right">
                  {totalState > 0 ? (
                    <button className="chip" type="button" onClick={() => setShowState((v) => !v)} aria-pressed={showState}>
                      {state.commitments.length} {t.due} · {state.open_loops.length} {t.waiting}
                    </button>
                  ) : null}
                  <button
                    className={`icon${pinned ? " is-on" : ""}`}
                    type="button"
                    title={pinned ? t.stayOpen : t.autoHide}
                    onClick={() => {
                      setPinned((v) => !v);
                      nudge();
                    }}
                  >
                    {pinned ? "◆" : "◇"}
                  </button>
                  <button className="icon" type="button" title={t.settings} onClick={() => setShowSettings(true)}>
                    ⚙
                  </button>
                </div>
              </header>

              {showState ? (
                <div className="state">
                  {state.commitments.map((c, i) => (
                    <div key={`c${i}`} className="state__row">
                      <span className="state__text">{c.text}</span>
                      <span className={`state__meta ${c.meta === "overdue" ? "is-over" : ""}`}>{c.meta}</span>
                    </div>
                  ))}
                  {state.open_loops.map((l, i) => (
                    <div key={`l${i}`} className="state__row">
                      <span className="state__text">{l.text}</span>
                      <span className="state__meta">{l.meta}</span>
                    </div>
                  ))}
                </div>
              ) : null}

              <div className="thread" ref={threadRef}>
                {msgs.length === 0 ? (
                  <div className="welcome">
                    <div className="welcome__t">{t.welcomeTitle}</div>
                    <div className="welcome__s">{t.welcomeSub}</div>
                    {IN_TAURI && status && !status.has_key ? <div className="welcome__key">{t.noKey}</div> : null}
                  </div>
                ) : (
                  msgs.map((m, i) => (
                    <div key={i} className={`msg msg--${m.role}`}>
                      {m.text}
                    </div>
                  ))
                )}
                {thinking ? <div className="msg msg--shogun msg--think">…</div> : null}
              </div>

              <div className="composer">
                <button className="composer__draft" type="button" onClick={draftAtCursor} title={t.draftTitle}>
                  ✎
                </button>
                <input
                  className="composer__input"
                  placeholder={t.ask}
                  value={input}
                  onFocus={nudge}
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      send();
                    }
                  }}
                />
                <button className="composer__send" type="button" onClick={send} disabled={!input.trim() || thinking}>
                  ↑
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function Settings(props: {
  appearance: Appearance;
  setAppearance: (a: Appearance) => void;
  pinned: boolean;
  setPinned: (v: boolean) => void;
  hasKey: boolean;
  onDone: () => void;
}): JSX.Element {
  const { appearance, setAppearance, pinned, setPinned, hasKey, onDone } = props;
  return (
    <div className="settings">
      <header className="settings__head">
        <span className="settings__title">{t.settings}</span>
        <button className="chip" type="button" onClick={onDone}>
          {t.done}
        </button>
      </header>

      <div className="settings__body">
        <section className="set">
          <div className="set__label">{t.appearance}</div>
          <div className="seg">
            {(["dark", "light", "auto"] as Appearance[]).map((a) => (
              <button
                key={a}
                type="button"
                className={`seg__opt${appearance === a ? " is-on" : ""}`}
                onClick={() => setAppearance(a)}
              >
                {a === "dark" ? t.dark : a === "light" ? t.light : t.auto}
              </button>
            ))}
          </div>
        </section>

        <section className="set">
          <div className="set__label">{t.behavior}</div>
          <div className="seg">
            <button type="button" className={`seg__opt${pinned ? " is-on" : ""}`} onClick={() => setPinned(true)}>
              {t.stayOpen}
            </button>
            <button type="button" className={`seg__opt${!pinned ? " is-on" : ""}`} onClick={() => setPinned(false)}>
              {t.autoHide}
            </button>
          </div>
          <div className="set__hint">{pinned ? t.stayOpenHint : t.autoHideHint}</div>
        </section>

        <section className="set">
          <div className="set__label">{t.draftShortcut}</div>
          <div className="set__row">
            <kbd>⌃</kbd>
            <kbd>⌥</kbd>
            <kbd>G</kbd>
          </div>
        </section>

        <section className="set">
          <div className="set__label">{t.key}</div>
          <div className={`set__hint${hasKey ? " is-ok" : ""}`}>{hasKey ? t.keyPresent : t.keyAbsent}</div>
        </section>
      </div>
    </div>
  );
}
