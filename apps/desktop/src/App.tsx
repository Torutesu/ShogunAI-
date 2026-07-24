import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Report webview-side failures to the terminal via Rust — a silent catch made real errors
// (missing window-API permissions) look like "the button does nothing".
function uiLog(msg: string): void {
  if (IN_TAURI) void invoke("ui_log", { msg }).catch(() => undefined);
}

// Explicit window drag on mouse-down. data-tauri-drag-region proved unreliable on device, so call
// startDragging() directly. Ignore drags that start on an interactive control (button/input).
function beginDrag(e: React.MouseEvent): void {
  if (!IN_TAURI || e.button !== 0) return;
  const el = e.target as HTMLElement;
  if (el.closest("button, input, a, [data-no-drag]")) return;
  // The visible surface is a NATIVE NSPanel hosting this webview — drag IT. The tao window is a
  // hidden shell, so getCurrentWindow().startDragging() would grab the wrong window.
  void invoke("start_panel_drag").catch(() =>
    getCurrentWindow()
      .startDragging()
      .catch((err) => uiLog(`startDragging failed: ${err}`)),
  );
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
  id: number;
  text: string;
  meta: string;
}
interface StateView {
  commitments: StateItem[];
  open_loops: StateItem[];
}

const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

// Window sizing. Collapsed = just the handle strip. Open (chat) starts as a wide short bar; the
// Settings view opens much taller so its stacked sections fit without feeling clipped. Both open
// views are user-resizable via the corner grip, and each remembers its own size across the
// Rust-driven respawns.
const W = 640;
const H_OPEN = 300;
const H_HANDLE = 44;
const H_SETTINGS = 520; // taller default so setting groups fit; body scrolls; clamped to screen
const MIN_W = 460;
const MIN_H = 240;
// Collapsed fallback, used only until the pill has been measured. The window is transparent, so
// any part of it that ISN'T the pill would still swallow clicks meant for the app underneath —
// the collapsed window is therefore shrunk to the pill's real bounds (see the measuring effect).
const W_HANDLE_FALLBACK = 260;

interface Size {
  w: number;
  h: number;
}

// Keep the panel inside the visible screen — it hangs from the top (notch), so cap it to the work
// area. availWidth/Height already exclude the Dock and menu bar; leave a small margin.
function maxSize(): Size {
  if (typeof window === "undefined") return { w: 1200, h: 800 };
  return {
    w: Math.max(MIN_W, Math.round(window.screen.availWidth - 40)),
    h: Math.max(MIN_H, Math.round(window.screen.availHeight - 40)),
  };
}
function clampSize(w: number, h: number): Size {
  const m = maxSize();
  return {
    w: Math.min(Math.max(Math.round(w), MIN_W), m.w),
    h: Math.min(Math.max(Math.round(h), MIN_H), m.h),
  };
}

const MOCK_STATUS: Status = { app: "com.apple.mail", commitments: 2, open_loops: 1, has_key: false };
const MOCK_STATE: StateView = {
  commitments: [
    { id: 1, text: "Send Alice the Q3 deck", meta: "overdue" },
    { id: 2, text: "Reply to the vendor about pricing", meta: "70% sure" },
  ],
  open_loops: [{ id: 1, text: "Waiting on legal sign-off", meta: "3d waiting" }],
};

function appName(bundle: string): string {
  if (!bundle) return t.yourScreen;
  const seg = bundle.split(".").pop() || bundle;
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}

async function applyPanelSize(w: number, h: number): Promise<void> {
  if (!IN_TAURI) return;
  try {
    // Resize the NATIVE panel (top edge anchored). Falls back to the tao window when the native
    // panel isn't in play (plain-window mode).
    await invoke("set_panel_size", { width: w, height: h });
  } catch {
    try {
      await getCurrentWindow().setSize(new LogicalSize(w, h));
    } catch (err) {
      // resize failure must not break the UI, but it must be VISIBLE in the log
      uiLog(`setSize failed: ${err}`);
    }
  }
}

// The Rust side RESPAWNS the whole window to move it onto the active Space (the only reliable way
// on this machine), which reloads the webview — so UI state lives in localStorage to survive it.
function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function saveJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* best-effort */
  }
}

export function App(): JSX.Element {
  // Always launch EXPANDED so the chat is visible immediately. (A stale collapsed flag in
  // localStorage used to leave the window at open-size while rendering only the handle — a big
  // empty panel showing just "reading …".) Minimize still collapses within the session.
  const [open, setOpen] = useState<boolean>(true);
  const [status, setStatus] = useState<Status | null>(IN_TAURI ? null : MOCK_STATUS);
  const [state, setState] = useState<StateView>(IN_TAURI ? { commitments: [], open_loops: [] } : MOCK_STATE);
  const [ctxApp, setCtxApp] = useState<string>("");
  const [msgs, setMsgs] = useState<Msg[]>(() => loadJson<Msg[]>("shogun.msgs", []));
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  const [appearance, setAppearance] = useState<Appearance>(() => loadJson<Appearance>("shogun.appearance", "auto"));

  // Persist across window respawns (the Rust side rebuilds the window to change Spaces).
  useEffect(() => saveJson("shogun.open", open), [open]);
  useEffect(() => saveJson("shogun.msgs", msgs.slice(-50)), [msgs]);
  useEffect(() => saveJson("shogun.appearance", appearance), [appearance]);
  const [showState, setShowState] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  // Open-view sizes are user-resizable (corner grip) and persist across the Rust-driven respawns.
  // Chat and Settings keep INDEPENDENT sizes — chat wants short+wide, Settings wants tall enough
  // for its stacked groups.
  const [chatSize, setChatSize] = useState<Size>(() => {
    const s = loadJson<Size>("shogun.size.chat", { w: W, h: H_OPEN });
    return clampSize(s.w, s.h);
  });
  const [setSize, setSetSize] = useState<Size>(() => {
    const s = loadJson<Size>("shogun.size.settings", { w: W, h: H_SETTINGS });
    return clampSize(s.w, s.h);
  });
  useEffect(() => saveJson("shogun.size.chat", chatSize), [chatSize]);
  useEffect(() => saveJson("shogun.size.settings", setSize), [setSize]);

  // Size the window to match the current view (handle / chat / settings). Pass explicit flags when
  // a state setter in the same handler hasn't committed yet (React batches updates).
  const sizeForView = useCallback(
    (opts?: { open?: boolean; settings?: boolean }): void => {
      const isOpen = opts?.open ?? open;
      const isSettings = opts?.settings ?? showSettings;
      // Collapsed: a provisional pill-sized window; the measuring effect below tightens it to the
      // pill's real bounds so the transparent remainder never eats clicks.
      if (!isOpen) void applyPanelSize(W_HANDLE_FALLBACK, H_HANDLE);
      else if (isSettings) void applyPanelSize(setSize.w, setSize.h);
      else void applyPanelSize(chatSize.w, chatSize.h);
    },
    [open, showSettings, chatSize, setSize],
  );
  // The boot/summon listeners live in a run-once effect; a ref keeps them calling the LATEST sizer
  // instead of a stale closure captured at mount.
  const sizeForViewRef = useRef(sizeForView);
  sizeForViewRef.current = sizeForView;

  // Live resize from the corner grip. During the drag we only resize the native panel (the webview
  // reflows via CSS — no React state churn), rAF-throttled so we don't flood the IPC bridge. The
  // per-view size is committed to state (and persisted) once on release.
  const liveSize = useRef<Size | null>(null);
  const raf = useRef<number | null>(null);
  const onResizeLive = useCallback((w: number, h: number): void => {
    const s = clampSize(w, h);
    liveSize.current = s;
    if (raf.current == null) {
      raf.current = requestAnimationFrame(() => {
        raf.current = null;
        const cur = liveSize.current;
        if (cur) void applyPanelSize(cur.w, cur.h);
      });
    }
  }, []);
  const onResizeCommit = useCallback((): void => {
    if (raf.current != null) {
      cancelAnimationFrame(raf.current);
      raf.current = null;
    }
    const s = liveSize.current;
    liveSize.current = null;
    if (!s) return;
    void applyPanelSize(s.w, s.h);
    if (showSettings) setSetSize(s);
    else setChatSize(s);
  }, [showSettings]);
  // Active agent provider, shown on the composer's model pill (mirrors Settings → Model).
  const [provider, setProvider] = useState<string>("anthropic");
  const threadRef = useRef<HTMLDivElement>(null);
  // Measured to size the collapsed window to the pill (see the measuring effect below).
  const handleRef = useRef<HTMLButtonElement>(null);

  const refreshLlm = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<{ provider: string; model: string }>("get_llm_settings")
      .then((s) => setProvider(s.provider))
      .catch(() => undefined);
  }, []);
  useEffect(refreshLlm, [refreshLlm]);

  useEffect(() => {
    // Always stamp the attribute so the theme is deterministic — styles.css carries an explicit
    // [data-appearance="dark"] block, so we no longer lean on bare :root defaults for dark.
    document.documentElement.setAttribute("data-appearance", appearance);
  }, [appearance]);

  // Start at the open size and prove the webview is alive.
  useEffect(() => {
    if (!IN_TAURI) return;
    sizeForViewRef.current({ open: true, settings: false });
    void invoke("interact", { kind: "boot" });
    const offs: Array<Promise<() => void>> = [];
    offs.push(listen<ContextPayload>("context", (e) => setCtxApp(e.payload.bundle_id || e.payload.title_masked || "")));
    // ⌃⌥N summon: Rust moved the window to this screen — also expand from the minimized handle.
    offs.push(
      listen("summon", () => {
        setOpen(true);
        sizeForViewRef.current({ open: true });
      }),
    );
    offs.push(
      listen<ClockSyncPayload>("clock_sync", (e) =>
        void invoke("clock_sync_ack", { seq: e.payload.seq, jsPerfMs: performance.now() }),
      ),
    );
    // Overlay spec: Escape closes the overlay (it stays hidden until summoned again).
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key === "Escape") void invoke("hide_panel").catch(() => undefined);
    };
    window.addEventListener("keydown", onEsc);
    return () => {
      window.removeEventListener("keydown", onEsc);
      offs.forEach((p) => void p.then((off) => off()));
    };
  }, []);

  const refreshState = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<Status>("shogun_status").then((s) => setStatus(s)).catch(() => undefined);
    void invoke<StateView>("shogun_state").then((s) => s && setState(s)).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!IN_TAURI) return;
    refreshState();
    const id = setInterval(refreshState, 3000);
    return () => clearInterval(id);
  }, [refreshState]);

  // Click a state row to resolve it (commitment → done, open loop → closed); refresh immediately.
  const resolveItem = (kind: "commitment" | "open_loop", id: number): void => {
    if (!IN_TAURI) return;
    // Optimistic: drop it from the list now so the click feels instant.
    setState((s) => ({
      commitments: kind === "commitment" ? s.commitments.filter((c) => c.id !== id) : s.commitments,
      open_loops: kind === "open_loop" ? s.open_loops.filter((l) => l.id !== id) : s.open_loops,
    }));
    void invoke("resolve_state_item", { kind, id }).then(refreshState).catch(() => refreshState());
  };

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
  }, [msgs, thinking, open]);

  const collapse = (): void => {
    setShowSettings(false);
    setOpen(false);
    sizeForView({ open: false });
  };
  const expand = (): void => {
    setOpen(true);
    sizeForView({ open: true });
  };
  const openSettings = (): void => {
    setShowSettings(true);
    sizeForView({ open: true, settings: true });
  };
  const closeSettings = (): void => {
    setShowSettings(false);
    sizeForView({ open: true, settings: false });
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
    // No key means the backend would answer from the echo mock, so say so directly rather than
    // round-tripping for a non-answer.
    if (status && !status.has_key) {
      finish(t.noKey);
      return;
    }
    void invoke<string>("shogun_chat", { message: q })
      .then((r) => finish(r || t.noAnswer))
      .catch((e) => finish(`${t.answerFailed}: ${e}`));
  }, [input, thinking, status]);

  const draftAtCursor = (): void => {
    if (IN_TAURI) void invoke("inline_at_cursor").catch(() => undefined);
  };

  const totalState = state.commitments.length + state.open_loops.length;
  const live = appName(ctxApp || status?.app || "");
  const providerLabel = PROVIDERS.find((p) => p.id === provider)?.label ?? t.model;

  // Collapsed: shrink the window to the pill's real bounds. The panel is transparent, so every
  // pixel of window that isn't the pill would still intercept clicks aimed at the app underneath
  // (an invisible dead zone across the top-left of the screen). Re-measured whenever the pill's
  // content — and therefore its width — changes.
  useEffect(() => {
    if (open) return;
    const el = handleRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return;
    // +1 guards against a fractional layout width being truncated into a clipped pill.
    void applyPanelSize(Math.ceil(r.width) + 1, Math.ceil(r.height) + 1);
  }, [open, live, state.commitments.length, state.open_loops.length]);

  // Collapsed: a clear, clickable handle hanging from the notch.
  if (!open) {
    return (
      <div className="stage stage--handle">
        <button className="handle" ref={handleRef} type="button" onClick={expand} title={t.openPanel}>
          <span className="handle__live">
            <span className="live__dot" />
            {t.reading} <b>{live}</b>
          </span>
          {state.commitments.length > 0 ? (
            <span className="handle__count">
              {state.commitments.length} {t.due}
            </span>
          ) : state.open_loops.length > 0 ? (
            <span className="handle__count">
              {state.open_loops.length} {t.waiting}
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
          <Settings
            appearance={appearance}
            setAppearance={setAppearance}
            hasKey={!!status?.has_key}
            stateCount={state.commitments.length + state.open_loops.length}
            onDone={() => {
              closeSettings();
              refreshLlm();
            }}
            onCleared={refreshState}
          />
        ) : (
          <>
            <header className="head" onMouseDown={beginDrag}>
              <div className="head__left">
                {/* The live source sits top-left, in the same spot the collapsed pill occupies, so
                    opening the panel doesn't make the indicator jump to the bottom. App NAME only —
                    never window titles or paths (no usernames leak into the UI). */}
                <span className="srcchip" title={`${t.reading} ${live}`}>
                  <span className="live__dot" />
                  {t.reading} <b>{live}</b>
                </span>
                {totalState > 0 ? (
                  <button className="chip" type="button" onClick={() => setShowState((v) => !v)} aria-pressed={showState}>
                    {state.commitments.length} {t.due} · {state.open_loops.length} {t.waiting}
                  </button>
                ) : null}
              </div>
              <div className="head__right">
                <button className="icon" type="button" title={t.settings} aria-label={t.settings} onClick={openSettings}>
                  ⚙︎
                </button>
                <button className="icon" type="button" title={t.minimize} aria-label={t.minimize} onClick={collapse}>
                  ▁
                </button>
                <button
                  className="icon icon--close"
                  type="button"
                  title={t.quitTitle}
                  aria-label={t.quitTitle}
                  onClick={() => {
                    if (IN_TAURI) void invoke("quit_app").catch((err) => uiLog(`quit failed: ${err}`));
                  }}
                >
                  ✕
                </button>
              </div>
            </header>

            {showState ? (
              <div className="state">
                {state.commitments.map((c) => (
                  <button key={`c${c.id}`} type="button" className="state__row" onClick={() => resolveItem("commitment", c.id)} title={t.resolveHint}>
                    <span className="state__check">✓</span>
                    <span className="state__text">{c.text}</span>
                    <span className={`state__meta ${c.meta === "overdue" ? "is-over" : ""}`}>{c.meta}</span>
                  </button>
                ))}
                {state.open_loops.map((l) => (
                  <button key={`l${l.id}`} type="button" className="state__row" onClick={() => resolveItem("open_loop", l.id)} title={t.resolveHint}>
                    <span className="state__check">✓</span>
                    <span className="state__text">{l.text}</span>
                    <span className="state__meta">{l.meta}</span>
                  </button>
                ))}
                {state.commitments.length + state.open_loops.length === 0 ? (
                  <div className="state__empty">{t.stateEmpty}</div>
                ) : null}
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
              {thinking ? (
                <div className="msg msg--shogun msg--think" aria-live="polite" aria-label="Thinking">
                  <span className="think__dot" />
                  <span className="think__dot" />
                  <span className="think__dot" />
                </div>
              ) : null}
            </div>

            <div className="composer">
              <div className="composer__card">
                {/* The live-source chip lives in the header (top-left), not here — see the note
                    there. The composer is just the field plus its action bar. */}
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
                <div className="composer__bar">
                  <button
                    className="composer__model"
                    type="button"
                    title={t.model}
                    onClick={openSettings}
                  >
                    {providerLabel} <span className="composer__caret" aria-hidden="true">⌄</span>
                  </button>
                  <div className="composer__tools">
                    <button
                      className="composer__draft"
                      type="button"
                      onClick={draftAtCursor}
                      title={t.draftTitle}
                      aria-label={t.draftTitle}
                    >
                      ✎
                    </button>
                    <button
                      className="composer__send"
                      type="button"
                      title={t.send}
                      aria-label={t.send}
                      onClick={send}
                      disabled={!input.trim() || thinking}
                    >
                      ↑
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </>
        )}
        <ResizeGrip
          current={() => (showSettings ? setSize : chatSize)}
          onResize={onResizeLive}
          onCommit={onResizeCommit}
        />
      </div>
    </div>
  );
}

// Corner grip that lets the user stretch the open panel. The native panel is a borderless NSPanel
// (no OS resize edges), so we drive `set_panel_size` from a pointer drag — the panel's top-left is
// anchored, so it grows down/right, which reads naturally for a window that hangs from the notch.
function ResizeGrip(props: {
  current: () => Size;
  onResize: (w: number, h: number) => void;
  onCommit: () => void;
}): JSX.Element {
  const { current, onResize, onCommit } = props;
  const start = useRef<{ x: number; y: number; w: number; h: number } | null>(null);
  const onDown = (e: React.PointerEvent): void => {
    e.preventDefault();
    e.stopPropagation();
    const s = current();
    start.current = { x: e.clientX, y: e.clientY, w: s.w, h: s.h };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onMove = (e: React.PointerEvent): void => {
    const s = start.current;
    if (!s) return;
    onResize(s.w + (e.clientX - s.x), s.h + (e.clientY - s.y));
  };
  const onUp = (e: React.PointerEvent): void => {
    if (!start.current) return;
    start.current = null;
    onCommit();
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* pointer may already be released */
    }
  };
  return (
    <div
      className="resizer"
      data-no-drag
      title={t.resizeHint}
      aria-hidden="true"
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
    />
  );
}

// First-layer connections (§6.9). Connect / disconnect a service and show its sync state. Talks to
// the Rust connector commands (connectors_list / connect_service / disconnect_service); the data
// layer stays in Rust (invariant 1) — this is presentation only.
type ConnState = "connected" | "needs_reauth" | "disconnected" | "coming_soon";
interface ServiceStatus {
  source: string; // "gmail" | "gcal" | "gdrive" | "slack" | "notion" | "github" | "linear"
  state: ConnState;
  last_sync_ms: number | null;
  has_endpoint: boolean;
}
const CONN_LABELS: Record<string, string> = {
  gmail: "Gmail",
  gcal: "Google Calendar",
  gdrive: "Google Drive",
  slack: "Slack",
  notion: "Notion",
  github: "GitHub",
  linear: "Linear",
};
const CONN_STATE_LABEL: Record<ConnState, string> = {
  connected: "Connected",
  needs_reauth: "Needs reauth",
  disconnected: "Not connected",
  coming_soon: "Coming soon",
};

function ConnectionsSection(): JSX.Element {
  const [rows, setRows] = useState<ServiceStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<ServiceStatus[]>("connectors_list")
      .then((r) => {
        setRows(r);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(refresh, [refresh]);

  const act = useCallback(
    (cmd: "connect_service" | "disconnect_service", source: string): void => {
      setBusy(source);
      setError(null);
      void invoke(cmd, { service: source })
        .then(refresh)
        .catch((e) => setError(String(e)))
        .finally(() => setBusy(null));
    },
    [refresh],
  );

  return (
    <section className="set">
      <div className="set__label">{t.connections}</div>
      <div className="set__hint">{t.connectionsHint}</div>
      {error ? <div className="set__hint is-err">{error}</div> : null}
      {rows.length === 0 ? (
        <div className="set__hint">{t.connectionsEmpty}</div>
      ) : (
        <div className="conns">
          {rows.map((r) => {
            const label = CONN_LABELS[r.source] ?? r.source;
            const canConnect = r.has_endpoint && r.state !== "coming_soon";
            const connected = r.state === "connected" || r.state === "needs_reauth";
            const stateMod =
              r.state === "connected" ? " is-ok" : r.state === "needs_reauth" ? " is-warn" : "";
            return (
              <div key={r.source} className="conn">
                <div className="conn__meta">
                  <span className="conn__name">{label}</span>
                  <span className={`conn__state${stateMod}`}>
                    {CONN_STATE_LABEL[r.state]}
                    {r.last_sync_ms ? ` · ${new Date(r.last_sync_ms).toLocaleTimeString()}` : ""}
                  </span>
                </div>
                {connected ? (
                  <button
                    className="keyrow__btn"
                    type="button"
                    disabled={busy === r.source}
                    onClick={() => act("disconnect_service", r.source)}
                  >
                    {busy === r.source ? "…" : t.disconnect}
                  </button>
                ) : (
                  <button
                    className="keyrow__btn"
                    type="button"
                    disabled={!canConnect || busy === r.source}
                    onClick={() => act("connect_service", r.source)}
                    title={canConnect ? "" : t.connectionsUnavailable}
                  >
                    {busy === r.source ? t.connecting : t.connect}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

// L3 confirmation queue (§6.6 / FR-AG-03, invariant 4). Every action that leaves the device stops
// here until the user presses the dedicated Confirm button — Enter alone must never confirm, and
// the FULL body is shown, never a summary. The queue itself lives in Rust (invariant 1).
interface ApprovalView {
  id: number;
  op_type: string;
  destination: string;
  full_body: string;
  route: string; // "direct" | "composio"
}

function ApprovalsSection(): JSX.Element | null {
  const [rows, setRows] = useState<ApprovalView[]>([]);
  const [busy, setBusy] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<ApprovalView[]>("list_approvals")
      .then((r) => {
        setRows(r);
        setError(null);
      })
      // A missing queue (connector runtime not started) must not paint an error banner over the
      // settings — the section simply stays empty.
      .catch(() => setRows([]));
  }, []);

  useEffect(() => {
    refresh();
    // Producers (agents) enqueue in the background, so poll while the panel is open.
    const id = setInterval(refresh, 5000);
    return () => clearInterval(id);
  }, [refresh]);

  const act = useCallback(
    (cmd: "confirm_send" | "reject_send", id: number): void => {
      setBusy(id);
      setError(null);
      void invoke<string>(cmd, { id })
        .then((outcome) => {
          if (typeof outcome === "string" && outcome.startsWith("failed:")) setError(outcome);
        })
        .catch((e) => setError(String(e)))
        .finally(() => {
          setBusy(null);
          refresh();
        });
    },
    [refresh],
  );

  // Nothing pending and nothing to report: stay out of the way.
  if (rows.length === 0 && !error) return null;

  return (
    <section className="set">
      <div className="set__label">{t.approvals}</div>
      <div className="set__hint">{t.approvalsHint}</div>
      {error ? <div className="set__hint is-err">{error}</div> : null}
      {rows.length === 0 ? (
        <div className="set__hint">{t.approvalsEmpty}</div>
      ) : (
        <div className="apprs">
          {rows.map((r) => (
            <div key={r.id} className="appr">
              <div className="appr__top">
                <span className="appr__op">{r.op_type}</span>
                <span className="appr__route">
                  {r.destination} · {r.route === "composio" ? t.approvalsVia : t.approvalsDirect}
                </span>
              </div>
              {/* FR-AG-03: the full body, never a summary. */}
              <pre className="appr__body">{r.full_body}</pre>
              <div className="appr__acts">
                <button
                  className="keyrow__btn keyrow__btn--go"
                  type="button"
                  disabled={busy === r.id}
                  onClick={() => act("confirm_send", r.id)}
                >
                  {busy === r.id ? "…" : t.approvalsConfirm}
                </button>
                <button
                  className="keyrow__btn"
                  type="button"
                  disabled={busy === r.id}
                  onClick={() => act("reject_send", r.id)}
                >
                  {t.approvalsReject}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

// Draft is not here: it fires on a bare ⌥ (Option) tap, which can't be a global shortcut and so
// isn't rebindable. Settings shows it as a fixed row.
const DEFAULT_BINDS: Record<string, string> = {
  summon: "Control+Alt+KeyN",
  quit: "Control+Alt+KeyQ",
};
const PROVIDERS: Array<{ id: string; label: string }> = [
  { id: "anthropic", label: "Claude API" },
  { id: "openrouter", label: "OpenRouter" },
  { id: "openai", label: "OpenAI" },
];
const SHORTCUT_ROWS: Array<{ action: string; label: string }> = [
  { action: "summon", label: t.summonShortcut },
  { action: "quit", label: t.quitShortcut },
];

/** "Control+Alt+KeyN" → ["⌃","⌥","N"] for <kbd> chips. */
function comboChips(combo: string): string[] {
  return combo.split("+").map((part) => {
    if (part === "Control") return "⌃";
    if (part === "Alt") return "⌥";
    if (part === "Shift") return "⇧";
    if (part === "Super") return "⌘";
    if (part.startsWith("Key")) return part.slice(3);
    if (part.startsWith("Digit")) return part.slice(5);
    return part;
  });
}

function Settings(props: {
  appearance: Appearance;
  setAppearance: (a: Appearance) => void;
  hasKey: boolean;
  stateCount: number;
  onDone: () => void;
  onCleared: () => void;
}): JSX.Element {
  const { appearance, setAppearance, hasKey, stateCount, onDone, onCleared } = props;
  // Clearing extracted state is destructive and context is foundational, so it is a deliberate
  // two-step: reveal a typed confirmation, and only a matching "CLEAR" enables the delete.
  const [confirming, setConfirming] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [cleared, setCleared] = useState(false);
  const canClear = confirmText.trim().toUpperCase() === "CLEAR";
  const clearMemory = (): void => {
    if (!IN_TAURI || !canClear) return;
    void invoke("clear_memory")
      .then(() => {
        setCleared(true);
        setConfirming(false);
        setConfirmText("");
        onCleared();
      })
      .catch(() => undefined);
  };
  const [binds, setBinds] = useState<Record<string, string>>(DEFAULT_BINDS);
  const [recording, setRecording] = useState<string | null>(null);
  const [keyErr, setKeyErr] = useState("");
  // BYOK key entry: the key goes straight to the macOS Keychain via Rust (never a file/DB/log).
  const [keyInput, setKeyInput] = useState("");
  const [keyState, setKeyState] = useState<boolean>(hasKey);
  const [keyMsg, setKeyMsg] = useState("");
  useEffect(() => setKeyState(hasKey), [hasKey]);
  // Agent-lane provider + model (non-secret; the key above is per-provider in the Keychain).
  const [provider, setProvider] = useState("anthropic");
  const [model, setModel] = useState("");
  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<{ provider: string; model: string }>("get_llm_settings")
      .then((s) => {
        setProvider(s.provider);
        setModel(s.model);
      })
      .catch(() => undefined);
  }, []);
  const applyLlm = (p: string, m: string): void => {
    const prev = { provider, model };
    setProvider(p);
    setModel(m);
    setKeyMsg("");
    if (IN_TAURI)
      void invoke("set_llm_settings", { provider: p, model: m }).catch((e) => {
        // Roll the UI back — an optimistic provider the backend never accepted would send the
        // next key save to the wrong Keychain account.
        setProvider(prev.provider);
        setModel(prev.model);
        setKeyMsg(String(e));
      });
  };

  const saveKey = (): void => {
    const k = keyInput.trim();
    if (!k) return;
    if (!IN_TAURI) {
      setKeyState(true);
      setKeyInput("");
      return;
    }
    // The provider is passed EXPLICITLY so the key always lands in the account the user sees
    // selected — never the backend's possibly-lagging idea of the current provider.
    void invoke("set_byok_key", { provider, key: k })
      .then(() => {
        setKeyState(true);
        setKeyInput("");
        setKeyMsg(t.keySaved);
      })
      .catch((e) => setKeyMsg(String(e)));
  };
  const removeKey = (): void => {
    if (!IN_TAURI) {
      setKeyState(false);
      return;
    }
    void invoke("clear_byok_key", { provider })
      .then(() => {
        setKeyState(false);
        setKeyMsg("");
      })
      .catch((e) => setKeyMsg(String(e)));
  };

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<Record<string, string>>("get_shortcuts")
      .then((b) => setBinds({ ...DEFAULT_BINDS, ...b }))
      .catch(() => undefined);
  }, []);
  useEffect(refresh, [refresh]);

  // Capture the new combo at the WINDOW level (capture phase). Relying on the recording button's
  // own focus proved fragile on device — clicks landed but keystrokes never did, so rebinding
  // looked broken. A window listener catches keys no matter where focus sits inside the panel.
  useEffect(() => {
    if (!recording) return;
    const action = recording;
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(null);
        setKeyErr("");
        return;
      }
      if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return; // modifier alone: keep waiting
      const mods = [
        e.ctrlKey && "Control",
        e.altKey && "Alt",
        e.shiftKey && "Shift",
        e.metaKey && "Super",
      ].filter(Boolean) as string[];
      if (mods.length === 0) {
        setKeyErr(t.needModifier);
        return;
      }
      const combo = [...mods, e.code].join("+");
      setRecording(null);
      setKeyErr("");
      if (!IN_TAURI) {
        setBinds((b) => ({ ...b, [action]: combo }));
        return;
      }
      void invoke("set_shortcut", { action, combo })
        .then(refresh)
        .catch((err) => setKeyErr(String(err)));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, refresh]);

  return (
    <div className="settings">
      <header className="settings__head">
        <span className="settings__title">{t.settings}</span>
        <button className="chip" type="button" onClick={onDone}>
          {t.done}
        </button>
      </header>
      <div className="settings__body">
        <ApprovalsSection />
        <ConnectionsSection />
        <section className="set">
          <div className="set__label" id="seg-appearance">{t.appearance}</div>
          <div className="seg" role="radiogroup" aria-labelledby="seg-appearance">
            {(["dark", "light", "auto"] as Appearance[]).map((a) => (
              <button
                key={a}
                type="button"
                role="radio"
                aria-checked={appearance === a}
                className={`seg__opt${appearance === a ? " is-on" : ""}`}
                onClick={() => setAppearance(a)}
              >
                {a === "dark" ? t.dark : a === "light" ? t.light : t.auto}
              </button>
            ))}
          </div>
        </section>
        <section className="set">
          <div className="set__label">{t.shortcuts}</div>
          {/* Draft is a fixed ⌥-tap trigger (a bare modifier can't be a global shortcut), shown
              here so it's discoverable but not presented as rebindable. */}
          <div className="keys">
            <span className="keys__name">{t.draftShortcut}</span>
            <span className="keys__combo keys__combo--fixed" title={t.draftFixedHint}>
              <kbd>⌥</kbd>
            </span>
          </div>
          {SHORTCUT_ROWS.map(({ action, label }) => (
            <div key={action} className="keys">
              <span className="keys__name">{label}</span>
              {recording === action ? (
                <button
                  className="keys__rec"
                  type="button"
                  onClick={() => {
                    setRecording(null);
                    setKeyErr("");
                  }}
                >
                  {t.recordHint}
                </button>
              ) : (
                <button
                  className="keys__btn"
                  type="button"
                  title={t.change}
                  onClick={() => {
                    setRecording(action);
                    setKeyErr("");
                  }}
                >
                  <span className="keys__combo">
                    {comboChips(binds[action] ?? "").map((c, i) => (
                      <kbd key={i}>{c}</kbd>
                    ))}
                  </span>
                </button>
              )}
            </div>
          ))}
          {keyErr ? <div className="set__hint is-err">{keyErr}</div> : null}
          <div className="set__hint">{t.shortcutHint}</div>
        </section>
        <section className="set">
          <div className="set__label" id="seg-provider">{t.model}</div>
          <div className="seg" role="radiogroup" aria-labelledby="seg-provider">
            {PROVIDERS.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={provider === p.id}
                className={`seg__opt${provider === p.id ? " is-on" : ""}`}
                onClick={() => {
                  // Model ids are provider-specific — carrying one across providers sends an
                  // invalid model to the new provider. Blank = the provider's default.
                  if (p.id !== provider) applyLlm(p.id, "");
                }}
              >
                {p.label}
              </button>
            ))}
          </div>
          <div className="keyrow">
            <input
              className="keyrow__input"
              placeholder={t.modelPlaceholder}
              value={model}
              autoComplete="off"
              onFocus={() => {
                if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
              }}
              onChange={(e) => setModel(e.target.value)}
              onBlur={() => applyLlm(provider, model)}
              onKeyDown={(e) => {
                if (e.key === "Enter") applyLlm(provider, model);
              }}
            />
          </div>
          <div className="set__hint">{t.modelHint}</div>
        </section>
        <section className="set">
          <div className="set__label">{t.key}</div>
          <div className={`set__hint${keyState ? " is-ok" : ""}`}>{keyState ? t.keyPresent : t.keyAbsent}</div>
          <div className="keyrow">
            <input
              className="keyrow__input"
              type="password"
              placeholder={t.keyPlaceholders[provider] ?? t.keyPlaceholders.anthropic}
              value={keyInput}
              autoComplete="off"
              onChange={(e) => setKeyInput(e.target.value)}
              onFocus={() => {
                // The nonactivating panel must become key before it takes keystrokes.
                if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") saveKey();
              }}
            />
            <button className="keyrow__btn" type="button" onClick={saveKey} disabled={!keyInput.trim()}>
              {t.keySave}
            </button>
            {keyState ? (
              <button className="keyrow__btn" type="button" onClick={removeKey}>
                {t.keyRemove}
              </button>
            ) : null}
          </div>
          {keyMsg ? <div className="set__hint">{keyMsg}</div> : null}
        </section>
        <section className="set">
          <div className="set__label">{t.memory}</div>
          <div className="set__hint">{t.memoryHint}</div>
          {!confirming ? (
            <div className="keyrow">
              <button
                className="keyrow__btn keyrow__btn--danger"
                type="button"
                onClick={() => {
                  setConfirming(true);
                  setCleared(false);
                }}
                disabled={stateCount === 0}
              >
                {stateCount > 0 ? `${t.memoryClear} (${stateCount})` : t.memoryClear}
              </button>
              {cleared ? <span className="set__hint is-ok">{t.memoryCleared}</span> : null}
            </div>
          ) : (
            <div className="confirm">
              <div className="set__hint is-err">{t.memoryConfirm.replace("{n}", String(stateCount))}</div>
              <div className="keyrow">
                <input
                  className="keyrow__input"
                  placeholder={t.memoryConfirmPlaceholder}
                  value={confirmText}
                  autoFocus
                  autoComplete="off"
                  onFocus={() => {
                    if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
                  }}
                  onChange={(e) => setConfirmText(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && canClear) clearMemory();
                    if (e.key === "Escape") {
                      setConfirming(false);
                      setConfirmText("");
                    }
                  }}
                />
                <button
                  className="keyrow__btn"
                  type="button"
                  onClick={() => {
                    setConfirming(false);
                    setConfirmText("");
                  }}
                >
                  {t.cancel}
                </button>
                <button className="keyrow__btn keyrow__btn--danger" type="button" onClick={clearMemory} disabled={!canClear}>
                  {t.memoryClearConfirm}
                </button>
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
