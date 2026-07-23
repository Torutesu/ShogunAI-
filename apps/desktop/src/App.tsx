import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Explicit window drag on mouse-down. data-tauri-drag-region proved unreliable on device, so call
// startDragging() directly. Ignore drags that start on an interactive control (button/input).
function beginDrag(e: React.MouseEvent): void {
  if (!IN_TAURI || e.button !== 0) return;
  const el = e.target as HTMLElement;
  if (el.closest("button, input, a, [data-no-drag]")) return;
  void getCurrentWindow().startDragging().catch(() => undefined);
}
import { t } from "./strings";

// SHOGUN panel. A visible, interactive window that hangs from the notch. Opening/closing is driven
// by direct clicks in the webview (reliable — no dependency on the CGEventTap hover path or a global
// hotkey), so it always works as long as the window renders. The Rust engine still feeds live
// `context` (the app you're reading) and the data lives in Rust (CLAUDE.md invariant 1); the webview
// owns only presentation. All-spaces/background float (NSPanel) is a separate, gated path.

type Appearance = "auto" | "light" | "dark";
type Msg = { role: "me" | "shogun"; text: string };

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

// Window heights (width stays fixed so the Rust top-centre pin holds). Collapsed = just the handle.
const W = 380;
const H_OPEN = 440;
const H_HANDLE = 52;

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

async function sizeWindow(open: boolean): Promise<void> {
  if (!IN_TAURI) return;
  try {
    await getCurrentWindow().setSize(new LogicalSize(W, open ? H_OPEN : H_HANDLE));
  } catch {
    /* resize is a nicety; a failure must not break the UI */
  }
}

export function App(): JSX.Element {
  const [open, setOpen] = useState(true);
  const [status, setStatus] = useState<Status | null>(IN_TAURI ? null : MOCK_STATUS);
  const [state, setState] = useState<StateView>(IN_TAURI ? { commitments: [], open_loops: [] } : MOCK_STATE);
  const [ctxApp, setCtxApp] = useState<string>("");
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  const [appearance, setAppearance] = useState<Appearance>("auto");
  const [showState, setShowState] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const threadRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = document.documentElement;
    if (appearance === "dark") el.removeAttribute("data-appearance");
    else el.setAttribute("data-appearance", appearance);
  }, [appearance]);

  // Start at the open size and prove the webview is alive.
  useEffect(() => {
    if (!IN_TAURI) return;
    void sizeWindow(true);
    void invoke("interact", { kind: "boot" });
    const offs: Array<Promise<() => void>> = [];
    offs.push(listen<ContextPayload>("context", (e) => setCtxApp(e.payload.bundle_id || e.payload.title_masked || "")));
    offs.push(
      listen<ClockSyncPayload>("clock_sync", (e) =>
        void invoke("clock_sync_ack", { seq: e.payload.seq, jsPerfMs: performance.now() }),
      ),
    );
    return () => offs.forEach((p) => void p.then((off) => off()));
  }, []);

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

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
  }, [msgs, thinking, open]);

  const collapse = (): void => {
    setShowSettings(false);
    setOpen(false);
    void sizeWindow(false);
  };
  const expand = (): void => {
    setOpen(true);
    void sizeWindow(true);
  };

  const send = useCallback((): void => {
    const q = input.trim();
    if (!q || thinking) return;
    setInput("");
    setMsgs((m) => [...m, { role: "me", text: q }]);
    setThinking(true);
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
  }, [input, thinking]);

  const draftAtCursor = (): void => {
    if (IN_TAURI) void invoke("inline_at_cursor").catch(() => undefined);
  };

  const totalState = state.commitments.length + state.open_loops.length;
  const live = appName(ctxApp || status?.app || "");

  // Collapsed: a clear, clickable handle hanging from the notch.
  if (!open) {
    return (
      <div className="stage stage--handle">
        <button className="handle" type="button" onClick={expand} title={t.settings}>
          <span className="handle__mark">⚔</span>
          <span className="handle__live">
            <span className="live__dot" />
            {t.reading} <b>{live}</b>
          </span>
          {totalState > 0 ? (
            <span className="handle__count">
              {state.commitments.length} {t.due}
            </span>
          ) : null}
        </button>
      </div>
    );
  }

  return (
    <div className="stage">
      <div className="panel">
        {showSettings ? (
          <Settings appearance={appearance} setAppearance={setAppearance} hasKey={!!status?.has_key} onDone={() => setShowSettings(false)} />
        ) : (
          <>
            <header className="head" onMouseDown={beginDrag}>
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
                <button className="icon" type="button" title={t.settings} onClick={() => setShowSettings(true)}>
                  ⚙
                </button>
                <button className="icon" type="button" title="Minimize" onClick={collapse}>
                  ▁
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
                onFocus={() => {
                  // A nonactivating NSPanel won't take keystrokes until it's made key. Ask Rust to
                  // makeKeyAndOrderFront so typing works (no-op on the plain-window fallback).
                  if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
                }}
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
  );
}

function Settings(props: {
  appearance: Appearance;
  setAppearance: (a: Appearance) => void;
  hasKey: boolean;
  onDone: () => void;
}): JSX.Element {
  const { appearance, setAppearance, hasKey, onDone } = props;
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
              <button key={a} type="button" className={`seg__opt${appearance === a ? " is-on" : ""}`} onClick={() => setAppearance(a)}>
                {a === "dark" ? t.dark : a === "light" ? t.light : t.auto}
              </button>
            ))}
          </div>
        </section>
        <section className="set">
          <div className="set__label">{t.shortcuts}</div>
          <div className="keys">
            <span className="keys__name">{t.summonShortcut}</span>
            <span className="keys__combo">
              <kbd>⌃</kbd>
              <kbd>⌥</kbd>
              <kbd>N</kbd>
            </span>
          </div>
          <div className="keys">
            <span className="keys__name">{t.draftShortcut}</span>
            <span className="keys__combo">
              <kbd>⌃</kbd>
              <kbd>⌥</kbd>
              <kbd>G</kbd>
            </span>
          </div>
          <div className="keys">
            <span className="keys__name">{t.quitShortcut}</span>
            <span className="keys__combo">
              <kbd>⌃</kbd>
              <kbd>⌥</kbd>
              <kbd>Q</kbd>
            </span>
          </div>
        </section>
        <section className="set">
          <div className="set__label">{t.key}</div>
          <div className={`set__hint${hasKey ? " is-ok" : ""}`}>{hasKey ? t.keyPresent : t.keyAbsent}</div>
        </section>

        <section className="set">
          <button
            className="quit"
            type="button"
            onClick={() => {
              if (IN_TAURI) void invoke("quit_app").catch(() => undefined);
            }}
          >
            {t.quit}
          </button>
        </section>
      </div>
    </div>
  );
}
