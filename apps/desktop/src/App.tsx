import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { uiLog } from "./uiLog";

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
import { SERVICE_ICONS } from "./serviceIcons";

// SHOGUN panel. A visible, interactive window that hangs from the notch. Opening/closing is driven
// by direct clicks in the webview (reliable — no dependency on the CGEventTap hover path or a global
// hotkey), so it always works as long as the window renders. The Rust engine still feeds live
// `context` (the app you're reading) and the data lives in Rust (CLAUDE.md invariant 1); the webview
// owns only presentation. All-spaces/background float (NSPanel) is a separate, gated path.

type Appearance = "auto" | "light" | "dark";
type Citation = { event_id: number; source: string; title: string | null };
type Msg = { role: "me" | "shogun"; text: string; citations?: Citation[] };
type ChatAnswer = { text: string; citations: Citation[] };

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

/** The pill's contents, pushed from Rust on every meeting transition and once a second while
 *  recording (FR-MT-09). Assembled in the core — the webview only draws it (invariant 1). */
interface MeetingView {
  state: "idle" | "offered" | "recording" | "wrapping";
  enabled: boolean;
  title: string | null;
  /** The app that triggered the offer — what "never for this app" applies to (FR-MT-02b). */
  app_bundle_id: string | null;
  elapsed_ms: number;
  countdown_ms: number;
}

/** mm:ss. Tabular figures in CSS keep the row from reflowing as the seconds tick. */
function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

interface Status {
  app: string;
  commitments: number;
  open_loops: number;
  has_key: boolean;
  /// The provider refused the key. Without this a ⌥-tap that 401s inserts nothing and looks
  /// exactly like a shortcut that does not work.
  key_rejected: boolean;
}
/** What the notch is doing about a ⌥-tap, pushed from Rust (see inline_source::InlineStatus). */
/** The notch state machine's own view, pushed on every transition (integrate.rs `state` event).
 *  `hover` is the preview level; `expanded` is the full panel. */
interface StatePayload {
  state: string;
  t0_mono_ns: number;
}

interface InlineStatus {
  phase: "drafting" | "inserted" | "no_context" | "no_key" | "key_rejected" | "failed";
  chars: number;
  detail: string | null;
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
const W = 560;
const H_OPEN = 360;
const H_HANDLE = 44;
/** How long the cursor must rest on the collapsed pill before it opens. Long enough that crossing
 *  the pill on the way somewhere else doesn't trigger it, short enough to feel immediate. */
const HOVER_DWELL_MS = 250;
/** Grace period after the pointer leaves an unpinned panel. Long enough to cross the gap to a
 *  control that overlaps it, short enough that the panel feels like it follows your attention. */
const AUTO_COLLAPSE_MS = 400;
/** How long the pill holds a ⌥-tap outcome before returning to the live source. Long enough to
 *  read, short enough that it never becomes something you have to dismiss. */
const INLINE_HOLD_MS = 2200;
const H_SETTINGS = 460; // taller default so setting groups fit; body scrolls; clamped to screen
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

const MOCK_STATUS: Status = {
  app: "com.apple.mail",
  commitments: 2,
  open_loops: 1,
  has_key: false,
  key_rejected: false,
};
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

/// How a resize should hold the panel in place.
/// - `center` (default): the panel hangs from the notch, so switching views must not walk it
///   sideways by half of every size change.
/// - `left`: the corner grip is a bottom-right drag, so the top-left has to stay put or the
///   pointer drifts away from the corner it grabbed.
type Anchor = "center" | "left";

async function applyPanelSize(w: number, h: number, anchor: Anchor = "center"): Promise<void> {
  if (!IN_TAURI) return;
  try {
    // Resize the NATIVE panel (top edge anchored). Falls back to the tao window when the native
    // panel isn't in play (plain-window mode).
    await invoke("set_panel_size", { width: w, height: h, anchor });
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
  const [showHistory, setShowHistory] = useState(false);
  /// The ⌥-tap's own feedback. The pill shows it briefly and then returns to the live source —
  /// this is a reply to a keystroke, not a status the user has to dismiss.
  const [inline, setInline] = useState<InlineStatus | null>(null);
  /// Pinned = the panel stays put. Unpinned = it withdraws as soon as the pointer leaves, which is
  /// the counterpart to opening on hover: the same gesture that summons it also dismisses it, so
  /// a glance costs no clicks at all. Persisted because the panel is respawned by Rust.
  const [pinned, setPinned] = useState<boolean>(() => loadJson<boolean>("shogun.pinned", true));
  /// Where this session's conversation begins in the persisted store. Captured once at mount, so
  /// anything above it is history rather than part of what you're doing now.
  const historyMark = useRef<number | null>(null);
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
  useEffect(() => saveJson("shogun.pinned", pinned), [pinned]);
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
        if (cur) void applyPanelSize(cur.w, cur.h, "left");
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
    void applyPanelSize(s.w, s.h, "left");
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

  const [meeting, setMeeting] = useState<MeetingView | null>(null);
  // The pill replaces the handle, so it needs its own ref for the collapsed-size measurement.
  const pillRef = useRef<HTMLDivElement>(null);

  // Start at the open size and prove the webview is alive.
  useEffect(() => {
    if (!IN_TAURI) return;
    sizeForViewRef.current({ open: true, settings: false });
    void invoke("interact", { kind: "boot" });
    const offs: Array<Promise<() => void>> = [];
    offs.push(listen<ContextPayload>("context", (e) => setCtxApp(e.payload.bundle_id || e.payload.title_masked || "")));
    // The pill is push-driven: Rust owns the lifecycle, the webview never decides that a meeting
    // has started (FR-MT-07). The first read covers a webview reload mid-meeting.
    offs.push(listen<MeetingView>("meeting", (e) => setMeeting(e.payload)));
    void invoke<MeetingView>("meeting_status").then(setMeeting).catch(() => undefined);
    offs.push(
      listen<InlineStatus>("inline", (e) => {
        setInline(e.payload);
        // `drafting` holds until the outcome replaces it — a spinner that timed itself out would
        // claim the draft had finished when it hadn't.
        if (e.payload.phase !== "drafting") {
          window.setTimeout(() => setInline(null), INLINE_HOLD_MS);
        }
      }),
    );
    // ⌃⌥N summon: Rust moved the window to this screen — also expand from the minimized handle.
    offs.push(
      listen("summon", () => {
        setOpen(true);
        sizeForViewRef.current({ open: true });
      }),
    );
    // The notch's own hover detection finally drives the panel. Until now the tracker ran, emitted
    // transitions, and nothing listened — opening was click-only, which is why hovering the notch
    // did nothing while hovering the collapsed pill worked.
    offs.push(
      listen<StatePayload>("state", (e) => {
        const st = e.payload.state;
        if (st === "hover" || st === "expanded") {
          // Never fight a user who pinned the panel open, and never re-open one they just closed
          // by hand — the tracker doesn't know about either.
          setOpen((cur) => {
            if (cur) return cur;
            sizeForViewRef.current({ open: true });
            return true;
          });
        } else if (st === "idle" || st === "hidden") {
          // Withdraw on the same rule as the pointer-leave path: pinned stays, work in progress
          // stays, everything else follows your attention.
          if (pinnedRef.current) return;
          if (inputRef.current.trim().length > 0 || thinkingRef.current) return;
          setOpen((cur) => {
            if (!cur) return cur;
            sizeForViewRef.current({ open: false });
            return false;
          });
        }
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

  // Withdraw when the pointer leaves — but only when doing so can't destroy work. Typing in the
  // composer, a half-written question, or an answer still streaming all mean the panel is in use
  // even though the cursor wandered off; collapsing then would throw away what the user was
  // doing. The delay covers the gap between the panel and anything it overlaps, so brushing past
  // an edge doesn't dismiss it.
  const leaveTimer = useRef<number | null>(null);
  const cancelAutoCollapse = useCallback((): void => {
    if (leaveTimer.current != null) {
      window.clearTimeout(leaveTimer.current);
      leaveTimer.current = null;
    }
  }, []);
  // Mirrors of the values the timeout has to consult. Reading them through `setState` updaters
  // looked like a way to get the latest value without adding deps, but React skips an updater
  // that returns the same state — which meant the nested collapse never ran and an unpinned panel
  // stayed open forever. A ref is the honest way to read "now" from inside a timer.
  const inputRef = useRef(input);
  inputRef.current = input;
  const thinkingRef = useRef(thinking);
  thinkingRef.current = thinking;
  // The state listener is registered once, so it needs the latest pin through a ref rather than
  // a captured value — otherwise an unpinned-at-mount panel would ignore the pin forever.
  const pinnedRef = useRef(pinned);
  pinnedRef.current = pinned;

  const onPanelLeave = useCallback((): void => {
    if (pinned) return;
    cancelAutoCollapse();
    leaveTimer.current = window.setTimeout(() => {
      leaveTimer.current = null;
      // Never collapse over work in progress: a focused composer, a half-written question, or an
      // answer still arriving all mean the panel is in use even though the cursor wandered off.
      const composerHasFocus = document.activeElement?.classList.contains("composer__input") ?? false;
      if (composerHasFocus || inputRef.current.trim().length > 0 || thinkingRef.current) return;
      setShowSettings(false);
      setOpen(false);
      sizeForViewRef.current({ open: false });
    }, AUTO_COLLAPSE_MS);
  }, [pinned, cancelAutoCollapse]);
  useEffect(() => cancelAutoCollapse, [cancelAutoCollapse]);

  const collapse = (): void => {
    setShowSettings(false);
    setOpen(false);
    sizeForView({ open: false });
  };
  const expand = (): void => {
    setOpen(true);
    sizeForView({ open: true });
  };

  // Hover-to-open. The pill opens on dwell, not on entry: Phase 0 lists hover misfire as an open
  // question, and opening the instant the cursor crosses the pill is exactly the failure mode —
  // the panel would fire while you were on your way to the menu bar. So we wait HOVER_DWELL_MS of
  // continuous hover and cancel the moment the pointer leaves. A pointer that is merely passing
  // through is gone long before the timer elapses.
  //
  // Deliberately not gated on cursor velocity: dwell alone is measurable in the spike, and adding a
  // second heuristic would make a No-Go answer harder to attribute. Revisit with the Phase 0 data.
  const hoverTimer = useRef<number | null>(null);
  const cancelHoverOpen = useCallback((): void => {
    if (hoverTimer.current != null) {
      window.clearTimeout(hoverTimer.current);
      hoverTimer.current = null;
    }
  }, []);
  const onHandleEnter = useCallback((): void => {
    cancelHoverOpen();
    hoverTimer.current = window.setTimeout(() => {
      hoverTimer.current = null;
      setOpen(true);
      sizeForViewRef.current({ open: true });
    }, HOVER_DWELL_MS);
  }, [cancelHoverOpen]);
  // A pending timer must not outlive the component (or a click that opens the panel first).
  useEffect(() => cancelHoverOpen, [cancelHoverOpen]);
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
    const finish = (text: string, citations?: Citation[]): void => {
      setMsgs((m) => [...m, { role: "shogun", text, citations }]);
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
    void invoke<ChatAnswer>("shogun_chat", { message: q })
      .then((r) => finish(r?.text || t.noAnswer, r?.citations))
      .catch((e) => finish(`${t.answerFailed}: ${e}`));
  }, [input, thinking, status]);

  // Fix the history boundary on first render, before anything is appended this session.
  if (historyMark.current === null) historyMark.current = msgs.length;
  const priorCount = historyMark.current;
  const visibleMsgs = showHistory ? msgs : msgs.slice(priorCount);

  const inlineLine = ((): { text: string; tone: "work" | "ok" | "warn" } | null => {
    if (!inline) return null;
    switch (inline.phase) {
      case "drafting":
        return { text: t.inlineDrafting, tone: "work" };
      case "inserted":
        return { text: t.inlineInserted, tone: "ok" };
      case "no_context":
        return { text: t.inlineNoField, tone: "warn" };
      case "key_rejected":
        return { text: t.inlineKeyRejected, tone: "warn" };
      // Nothing was written at the caret, so this has to read as a setup step rather than a
      // failure — the tap did nothing, and the reason is one field away in settings.
      case "no_key":
        return { text: t.inlineNoKey, tone: "warn" };
      default:
        return { text: t.inlineFailed, tone: "warn" };
    }
  })();

  const totalState = state.commitments.length + state.open_loops.length;
  const live = appName(ctxApp || status?.app || "");
  const providerLabel = PROVIDERS.find((p) => p.id === provider)?.label ?? t.model;

  // Collapsed: shrink the window to the pill's real bounds. The panel is transparent, so every
  // pixel of window that isn't the pill would still intercept clicks aimed at the app underneath
  // (an invisible dead zone across the top-left of the screen). Re-measured whenever the pill's
  // content — and therefore its width — changes.
  useEffect(() => {
    if (open) return;
    // Whichever of the two is on screen: the ordinary handle, or the meeting pill standing in
    // for it.
    const el = handleRef.current ?? pillRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return;
    // +1 guards against a fractional layout width being truncated into a clipped pill.
    void applyPanelSize(Math.ceil(r.width) + 1, Math.ceil(r.height) + 1);
  }, [
    open,
    live,
    state.commitments.length,
    state.open_loops.length,
    meeting?.state,
    meeting?.title,
    meeting?.elapsed_ms,
    meeting?.countdown_ms,
  ]);

  const meetingLive =
    meeting?.enabled && (meeting.state === "offered" || meeting.state === "recording")
      ? meeting
      : null;

  // Collapsed: a clear, clickable handle hanging from the notch.
  if (!open) {
    // A meeting in progress outranks the ordinary handle (FR-MT-09).
    if (meetingLive) {
      return (
        <div className="stage stage--handle">
          <div ref={pillRef}>
            <MeetingPill view={meetingLive} />
          </div>
        </div>
      );
    }
    return (
      <div className="stage stage--handle">
        <button
          className="handle"
          ref={handleRef}
          type="button"
          onClick={() => {
            cancelHoverOpen();
            expand();
          }}
          onPointerEnter={onHandleEnter}
          onPointerLeave={cancelHoverOpen}
          title={t.openPanel}
        >
          {inlineLine ? (
            <span className={`handle__live inline--${inlineLine.tone}`}>
              <span className={`inline__dot inline__dot--${inlineLine.tone}`} />
              {inlineLine.text}
            </span>
          ) : (
          <span className="handle__live">
            <span className="live__dot" />
            {t.reading} <b>{live}</b>
          </span>
          )}
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
      {meetingLive ? <MeetingPill view={meetingLive} /> : null}
      <div className="panel" onPointerEnter={cancelAutoCollapse} onPointerLeave={onPanelLeave}>
        {showSettings ? (
          <Settings
            appearance={appearance}
            setAppearance={setAppearance}
            hasKey={!!status?.has_key}
            keyRejected={!!status?.key_rejected}
            stateCount={state.commitments.length + state.open_loops.length}
            onDone={() => {
              closeSettings();
              refreshLlm();
            }}
            onCleared={refreshState}
          />
        ) : meetingLive?.state === "recording" ? (
          <MeetingNote />
        ) : (
          <>
            <header className="head" onMouseDown={beginDrag}>
              <div className="head__left">
                {/* The live source sits top-left, in the same spot the collapsed pill occupies, so
                    opening the panel doesn't make the indicator jump to the bottom. App NAME only —
                    never window titles or paths (no usernames leak into the UI). */}
                {inlineLine ? (
                  <span className={`srcchip inline--${inlineLine.tone}`}>
                    <span className={`inline__dot inline__dot--${inlineLine.tone}`} />
                    {inlineLine.text}
                  </span>
                ) : (
                  <span className="srcchip" title={`${t.reading} ${live}`}>
                    <span className="live__dot" />
                    {t.reading} <b>{live}</b>
                  </span>
                )}
                {totalState > 0 ? (
                  <button className="chip" type="button" onClick={() => setShowState((v) => !v)} aria-pressed={showState}>
                    {state.commitments.length} {t.due} · {state.open_loops.length} {t.waiting}
                  </button>
                ) : null}
              </div>
              <div className="head__right">
                <button
                  className={`icon${pinned ? " icon--on" : ""}`}
                  type="button"
                  title={pinned ? t.unpin : t.pin}
                  aria-label={pinned ? t.unpin : t.pin}
                  aria-pressed={pinned}
                  onClick={() => setPinned((v) => !v)}
                >
                  {/* A pin that leans when unpinned — the state is legible without reading the
                      tooltip, which matters for a control that changes how the panel behaves. */}
                  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                       stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"
                       style={pinned ? undefined : { transform: "rotate(45deg)" }}>
                    <path d="M9 4h6l-1 6 3 3H7l3-3-1-6z" />
                    <path d="M12 13v7" />
                  </svg>
                </button>
                {/* Only offered when there is a backlog to open — an always-present control for an
                    empty history is a button that does nothing most of the time. */}
                {priorCount > 0 ? (
                  <button
                    className="icon"
                    type="button"
                    title={t.history}
                    aria-label={t.history}
                    aria-pressed={showHistory}
                    onClick={() => setShowHistory((v) => !v)}
                  >
                    ⏱
                  </button>
                ) : null}
                {/* The panel is for a glance and a keystroke; anything you want to sit and read —
                    the brief, health, memory, the run log — lives in the Full UI window. */}
                <button
                  className="icon"
                  type="button"
                  title={t.openFullUi}
                  aria-label={t.openFullUi}
                  onClick={() => {
                    if (IN_TAURI) void invoke("open_full_ui").catch((err) => uiLog(`open_full_ui failed: ${err}`));
                  }}
                >
                  ⤢
                </button>
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
              {visibleMsgs.length === 0 ? (
                <div className="welcome">
                  <div className="welcome__t">{t.welcomeTitle}</div>
                  <div className="welcome__s">{t.welcomeSub}</div>
                  {IN_TAURI && status && !status.has_key ? <div className="welcome__key">{t.noKey}</div> : null}
                </div>
              ) : (
                visibleMsgs.map((m, i) => (
                  <div key={i} className={`msg msg--${m.role}`}>
                    {m.text}
                    {/* What the answer was grounded in — so the user can check SHOGUN rather
                        than take its word. */}
                    {m.citations && m.citations.length > 0 ? (
                      <div className="cites">
                        <span className="cites__label">{t.sources}</span>
                        {m.citations.slice(0, 4).map((c) => (
                          <span key={c.event_id} className="cite" title={c.title ?? c.source}>
                            {c.title?.trim() || c.source}
                          </span>
                        ))}
                      </div>
                    ) : null}
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
                  {/* No draft button here. Drafting is the ⌥-tap gesture — a button that
                      duplicates it adds a control without adding a capability, and the composer
                      is the one place that has to stay uncluttered. */}
                  <div className="composer__tools">
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

/** The meeting note (FR-MT-10). Expanded panel is notes-only while recording. */
function MeetingNote(): JSX.Element {
  const [body, setBody] = useState("");
  const [status, setStatus] = useState<"idle" | "saved" | "failed">("idle");
  const latest = useRef("");

  const save = (text: string): void => {
    if (!IN_TAURI) return;
    void invoke("meeting_save_note", { body: text })
      .then(() => setStatus("saved"))
      .catch(() => setStatus("failed"));
  };

  useEffect(() => {
    latest.current = body;
  }, [body]);

  useEffect(() => {
    if (!body) return;
    const id = window.setTimeout(() => save(body), 800);
    return () => window.clearTimeout(id);
  }, [body]);

  useEffect(
    () => () => {
      if (latest.current) save(latest.current);
    },
    [],
  );

  return (
    <div className="mnote">
      <textarea
        className="mnote__area"
        value={body}
        placeholder={t.meetingNotePlaceholder}
        onChange={(e) => {
          setBody(e.target.value);
          setStatus("idle");
        }}
        onFocus={() => void invoke("focus_field", { focused: true }).catch(() => undefined)}
        onBlur={() => void invoke("focus_field", { focused: false }).catch(() => undefined)}
      />
      <div className="mnote__status">
        {status === "saved" ? t.meetingNotesSaved : status === "failed" ? t.meetingNotesFailed : ""}
      </div>
    </div>
  );
}

/** The meeting pill (FR-MT-08/09). Replaces the handle while offered or recording. */
function MeetingPill({ view }: { view: MeetingView }): JSX.Element {
  const title = view.title?.trim() || t.meetingUntitled;

  if (view.state === "offered") {
    return (
      <div className="mpill mpill--offer">
        <span className="mpill__title">{title}</span>
        <span className="mpill__count">
          {t.meetingStarting} {Math.ceil(view.countdown_ms / 1000)}s
        </span>
        <span className="mpill__acts">
          {view.app_bundle_id ? (
            <button
              type="button"
              className="mpill__btn mpill__btn--quiet"
              onClick={() =>
                void invoke("meeting_exclude_app", { bundleId: view.app_bundle_id }).catch(
                  () => undefined,
                )
              }
            >
              {t.meetingNeverThisApp}
            </button>
          ) : null}
          <button
            type="button"
            className="mpill__btn"
            onClick={() => void invoke("meeting_not_now").catch(() => undefined)}
          >
            {t.meetingNotNow}
          </button>
          <button
            type="button"
            className="mpill__btn mpill__btn--go"
            onClick={() => void invoke("meeting_start").catch(() => undefined)}
          >
            {t.meetingStart}
          </button>
        </span>
      </div>
    );
  }

  return (
    <div className="mpill">
      <span className="mpill__label">{t.meetingNotes}</span>
      <span className="mpill__time">{clock(view.elapsed_ms)}</span>
      <span className="mpill__title">{title}</span>
      <button
        type="button"
        className="mpill__btn mpill__btn--stop"
        onClick={() => void invoke("meeting_stop").catch(() => undefined)}
      >
        {t.meetingStop}
      </button>
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
// Brand marks, inlined from simple-icons at build time (see scripts/generate-service-icons.mjs).
// Services that project has removed on trademark request — Slack, OpenAI — fall back to a lettered
// disc in the service's own colour rather than an approximated logo.
const CONN_FALLBACK_TINT: Record<string, string> = {
  slack: "#611f69",
  openai: "#74aa9c",
};

/// Perceived luminance of a #rrggbb colour, 0..1 (Rec. 709 coefficients).
function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  return (0.2126 * r + 0.7152 * g + 0.0587 * b) / 255;
}

/// A service's mark: the real logo where we have one, a lettered disc where we don't.
///
/// Brand colours are used as-is except at the extremes — Notion and GitHub are near-black, which
/// disappears on the dark panel — where the mark falls back to the foreground colour so it stays
/// legible in whichever theme is showing.
function ServiceMark(props: { source: string; label: string }): JSX.Element {
  const icon = SERVICE_ICONS[props.source];
  const raw = icon?.hex ?? CONN_FALLBACK_TINT[props.source] ?? "";
  const lum = raw ? luminance(raw) : 0.5;
  const tint = !raw || lum < 0.16 || lum > 0.9 ? "var(--ink)" : raw;
  return (
    <span className="conn__mark" style={{ "--tint": tint } as React.CSSProperties} aria-hidden="true">
      {icon ? (
        <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor" role="presentation">
          <path d={icon.path} />
        </svg>
      ) : (
        props.label.charAt(0)
      )}
    </span>
  );
}

const CONN_STATE_LABEL: Record<ConnState, string> = {
  connected: "Connected",
  needs_reauth: "Needs reauth",
  disconnected: "Not connected",
  coming_soon: "Coming soon",
};

// The nightly cycle's result (FR-DC-06). Shown because the work happens while nobody is watching:
// without this, "did anything happen last night" is unanswerable, and a run that has been quietly
// failing for a week looks exactly like one that never had anything to do.
interface DreamStatus {
  indicator: "normal" | "amber" | "red";
  batch_lane: boolean;
  last_kind: "full" | "degraded" | null;
  last_cycle_id: string | null;
  last_succeeded: boolean;
  last_ended_at: number;
  jobs_done: number;
  jobs_failed: number;
  duration_ms: number;
  events_processed: number;
  state_changes: number;
  chunks_sent: number;
  done_tonight: boolean;
}

/** `20260724` → a date the reader recognises. */
function cycleDate(id: string | null): string {
  if (!id) return "";
  const m = /^(\d{4})(\d{2})(\d{2})/.exec(id);
  return m ? `${m[1]}-${m[2]}-${m[3]}` : id;
}

function DreamSection(): JSX.Element {
  const [status, setStatus] = useState<DreamStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectKkState, setSelectKkState] = useState(false);
  const [selectKkInput, setSelectKkInput] = useState("");
  const [selectKkMsg, setSelectKkMsg] = useState("");

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<DreamStatus>("dream_status").then(setStatus).catch(() => undefined);
    void invoke<boolean>("select_kk_configured")
      .then(setSelectKkState)
      .catch(() => undefined);
  }, []);
  useEffect(refresh, [refresh]);

  const saveSelectKk = (): void => {
    const k = selectKkInput.trim();
    if (!k || !IN_TAURI) return;
    void invoke("set_select_kk_key", { key: k })
      .then(() => {
        setSelectKkState(true);
        setSelectKkInput("");
        setSelectKkMsg(t.selectKkSaved);
      })
      .catch((e) => setSelectKkMsg(String(e)));
  };

  const removeSelectKk = (): void => {
    if (!IN_TAURI) {
      setSelectKkState(false);
      return;
    }
    void invoke("clear_select_kk_key")
      .then(() => {
        setSelectKkState(false);
        setSelectKkMsg("");
      })
      .catch((e) => setSelectKkMsg(String(e)));
  };

  const runNow = (): void => {
    setBusy(true);
    setError(null);
    void invoke("run_dream_now")
      .then(refresh)
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  // The headline is the state worth acting on, not a job count.
  const headline = ((): { text: string; cls: string } => {
    if (!status || !status.last_cycle_id) return { text: t.dreamNever, cls: "" };
    if (status.indicator === "red") return { text: t.dreamAttention, cls: " is-warn" };
    if (status.indicator === "amber" || !status.last_succeeded) {
      return { text: t.dreamCarried, cls: " is-warn" };
    }
    return { text: `${t.dreamOk} ${cycleDate(status.last_cycle_id)}`, cls: " is-ok" };
  })();

  return (
    <section className="set">
      <div className="set__label">{t.dream}</div>
      <div className="set__hint">{t.dreamHint}</div>
      {error ? <div className="set__hint is-err">{error}</div> : null}
      <div className="conn">
        <div className="conn__meta">
          <span className={`conn__state${headline.cls}`}>{headline.text}</span>
          {status?.last_cycle_id ? (
            <span className="conn__state">
              {status.events_processed} {t.dreamEvents} · {status.state_changes} {t.dreamChanges}
              {status.chunks_sent > 0 ? ` · ${status.chunks_sent} ${t.dreamChunks}` : ""}
            </span>
          ) : null}
          {status && !status.batch_lane ? (
            <span className="conn__state">{t.dreamLocal}</span>
          ) : null}
        </div>
        <button className="keyrow__btn" type="button" disabled={busy} onClick={runNow}>
          {busy ? t.dreamRunning : t.dreamRunNow}
        </button>
      </div>
      <div className="set__hint">{t.selectKkKey}</div>
      <div className={`set__hint${selectKkState ? " is-ok" : ""}`}>
        {selectKkState ? t.selectKkPresent : t.selectKkAbsent}
      </div>
      <div className="set__hint">{t.selectKkHint}</div>
      <div className="keyrow">
        <input
          className="keyrow__input"
          type="password"
          placeholder={t.selectKkPlaceholder}
          value={selectKkInput}
          autoComplete="off"
          onChange={(e) => setSelectKkInput(e.target.value)}
          onFocus={() => {
            if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") saveSelectKk();
          }}
        />
        <button
          className="keyrow__btn"
          type="button"
          onClick={saveSelectKk}
          disabled={!selectKkInput.trim()}
        >
          {t.keySave}
        </button>
        {selectKkState ? (
          <button className="keyrow__btn" type="button" onClick={removeSelectKk}>
            {t.keyRemove}
          </button>
        ) : null}
      </div>
      {selectKkMsg ? <div className="set__hint">{selectKkMsg}</div> : null}
    </section>
  );
}

// AI coding-tool transcripts. Opt-in: a session log is a transcript of the user's work, so
// nothing is read until they say so.
/** Meeting notes: tier (a) of the three ways to say no, plus the disclosure (FR-MT-01/02/03).
 *
 *  Off is the shipped default and the initial render, so a settings screen that fails to reach
 *  the backend shows "Off" rather than briefly claiming the feature is on. */
function MeetingSection(): JSX.Element {
  const [on, setOn] = useState(false);
  const [micOnly, setMicOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  const [excluded, setExcluded] = useState<string[]>([]);

  const load = (): void => {
    if (!IN_TAURI) return;
    void invoke<{ enabled: boolean; excluded_apps: string[]; allow_mic_only_detect?: boolean }>(
      "get_meeting_settings",
    )
      .then((s) => {
        setOn(s.enabled);
        setMicOnly(s.allow_mic_only_detect ?? false);
        setExcluded(s.excluded_apps ?? []);
      })
      .catch(() => undefined);
  };

  useEffect(load, []);

  const toggle = (next: boolean): void => {
    if (!IN_TAURI) {
      setOn(next);
      return;
    }
    setBusy(true);
    setOn(next);
    void invoke("set_meeting_enabled", { enabled: next })
      .then(load)
      .catch(() => setOn(!next))
      .finally(() => setBusy(false));
  };

  const toggleMicOnly = (next: boolean): void => {
    if (!IN_TAURI) {
      setMicOnly(next);
      return;
    }
    setMicOnly(next);
    void invoke("set_meeting_allow_mic_only", { allow: next })
      .then(load)
      .catch(() => setMicOnly(!next));
  };

  return (
    <section className="set">
      <div className="set__label" id="seg-meeting">{t.meetingSection}</div>
      <div className="seg" role="radiogroup" aria-labelledby="seg-meeting">
        <button
          type="button"
          role="radio"
          aria-checked={on}
          disabled={busy}
          className={`seg__opt${on ? " is-on" : ""}`}
          onClick={() => toggle(true)}
        >
          {t.meetingOn}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!on}
          disabled={busy}
          className={`seg__opt${!on ? " is-on" : ""}`}
          onClick={() => toggle(false)}
        >
          {t.meetingOff}
        </button>
      </div>
      <div className="set__hint">{t.meetingHint}</div>
      {on ? (
        <div className="set">
          <label className="set__row">
            <input
              type="checkbox"
              checked={micOnly}
              onChange={(e) => toggleMicOnly(e.target.checked)}
            />
            <span>{t.meetingMicOnly}</span>
          </label>
          <div className="set__hint">{t.meetingMicOnlyHint}</div>
        </div>
      ) : null}
      {/* Tier (b), undoable. An exclusion added by an impatient tap during a meeting would
          otherwise become a permanent blind spot with no way back (FR-MT-02b). */}
      {on ? (
        <div className="mexcl">
          <div className="mexcl__label">{t.meetingExcluded}</div>
          {excluded.length === 0 ? (
            <div className="set__hint">{t.meetingExcludedEmpty}</div>
          ) : (
            <ul className="mexcl__list">
              {excluded.map((id) => (
                <li key={id} className="mexcl__row">
                  <span className="mexcl__id">{id}</span>
                  <button
                    type="button"
                    className="mexcl__rm"
                    onClick={() =>
                      void invoke("meeting_include_app", { bundleId: id })
                        .then(load)
                        .catch(() => undefined)
                    }
                  >
                    {t.meetingExcludedRemove}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
      {/* Kept visible whether the feature is on or off: someone deciding whether to turn it on
          needs this more than someone who already has (FR-MT-03). */}
      <div className="set__hint set__hint--quiet">{t.meetingDisclosure}</div>
    </section>
  );
}

function VisualRecallSection(): JSX.Element {
  const [on, setOn] = useState(false);
  const [busy, setBusy] = useState(false);
  type RecallStatus = {
    enabled: boolean;
    events_24h: number;
    frames_count: number;
    recent: {
      ts: number;
      app: string | null;
      window: string | null;
      chars: number;
      excerpt: string;
    }[];
  };
  const [status, setStatus] = useState<RecallStatus | null>(null);

  const refreshStatus = (): void => {
    if (!IN_TAURI) return;
    void invoke<RecallStatus>("get_visual_recall_status")
      .then(setStatus)
      .catch(() => undefined);
  };

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<{ enabled: boolean }>("get_visual_recall_settings")
      .then((s) => setOn(s.enabled))
      .catch(() => undefined);
    refreshStatus();
    const id = window.setInterval(refreshStatus, 12_000);
    return () => window.clearInterval(id);
  }, []);

  const toggle = (next: boolean): void => {
    if (!IN_TAURI) {
      setOn(next);
      return;
    }
    setBusy(true);
    setOn(next);
    void invoke("set_visual_recall_enabled", { enabled: next })
      .then(() => refreshStatus())
      .catch(() => setOn(!next))
      .finally(() => setBusy(false));
  };

  const openBrowse = (): void => {
    if (!IN_TAURI) return;
    void invoke("open_visual_recall").catch(() => undefined);
  };

  const latest = status?.recent[0];
  const statusLine = !on
    ? t.visualRecallStatusOff
    : latest
      ? t.visualRecallStatusLive(
          latest.chars,
          latest.app ?? "an app",
          latest.window ?? "",
        )
      : t.visualRecallStatusIdle;

  return (
    <section className="set">
      <div className="set__label" id="seg-visual-recall">{t.visualRecallSection}</div>
      <div className="seg" role="radiogroup" aria-labelledby="seg-visual-recall">
        <button
          type="button"
          role="radio"
          aria-checked={on}
          disabled={busy}
          className={`seg__opt${on ? " is-on" : ""}`}
          onClick={() => toggle(true)}
        >
          {t.visualRecallOn}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!on}
          disabled={busy}
          className={`seg__opt${!on ? " is-on" : ""}`}
          onClick={() => toggle(false)}
        >
          {t.visualRecallOff}
        </button>
      </div>
      <div className="set__hint">{t.visualRecallHint}</div>
      <button type="button" className="vr-launch" onClick={openBrowse}>
        <span className="vr-launch__glyph" aria-hidden="true">⤢</span>
        <span className="vr-launch__body">
          <span className="vr-launch__title">{t.visualRecallBrowse}</span>
          <span className="vr-launch__sub">{t.visualRecallBrowseSub}</span>
        </span>
        {status && status.frames_count > 0 ? (
          <span className="vr-launch__badge">{status.frames_count}</span>
        ) : null}
        <span className="vr-launch__arrow" aria-hidden="true">→</span>
      </button>
      <div className="set__hint set__hint--quiet">{statusLine}</div>
      <div className="set__hint set__hint--quiet">{t.visualRecallDisclosure}</div>
    </section>
  );
}

function AiSessionsSection(): JSX.Element {
  const [on, setOn] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<boolean>("get_ai_session_import").then(setOn).catch(() => undefined);
  }, []);

  const toggle = (next: boolean): void => {
    if (!IN_TAURI) {
      setOn(next);
      return;
    }
    setBusy(true);
    void invoke<boolean>("set_ai_session_import", { enabled: next })
      .then(setOn)
      .catch(() => undefined)
      .finally(() => setBusy(false));
  };

  return (
    <section className="set">
      <div className="set__label" id="seg-ai-sessions">{t.aiSessions}</div>
      <div className="seg" role="radiogroup" aria-labelledby="seg-ai-sessions">
        <button
          type="button"
          role="radio"
          aria-checked={on}
          disabled={busy}
          className={`seg__opt${on ? " is-on" : ""}`}
          onClick={() => toggle(true)}
        >
          {t.aiSessionsOn}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!on}
          disabled={busy}
          className={`seg__opt${!on ? " is-on" : ""}`}
          onClick={() => toggle(false)}
        >
          {t.aiSessionsOff}
        </button>
      </div>
      <div className="set__hint">{t.aiSessionsHint}</div>
    </section>
  );
}

// ---- Composio sending settings (opt-in, FR-C2-02 / FR-C2-03) -----------------------------------

interface ComposioSettingsView {
  has_key: boolean;
  key_last4: string;
  draft_stop: boolean;
  consent_acknowledged: boolean;
  user_id: string;
}

function ComposioSection(): JSX.Element {
  const [settings, setSettings] = useState<ComposioSettingsView>({
    has_key: false,
    key_last4: "",
    draft_stop: true,
    consent_acknowledged: false,
    user_id: "",
  });
  const [keyInput, setKeyInput] = useState("");
  const [userIdInput, setUserIdInput] = useState("");
  const [err, setErr] = useState("");
  // Local checkboxes for the consent disclosure flow
  const [check1, setCheck1] = useState(false);
  const [check2, setCheck2] = useState(false);
  const [check3, setCheck3] = useState(false);

  const refreshSettings = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<ComposioSettingsView>("composio_settings")
      .then((s) => {
        setSettings(s);
        setErr("");
      })
      .catch((e) => setErr(String(e)));
  }, []);

  useEffect(refreshSettings, [refreshSettings]);

  // Keep the user ID input field in sync with the stored value.
  useEffect(() => { setUserIdInput(settings.user_id); }, [settings.user_id]);

  const saveUserId = (): void => {
    const id = userIdInput.trim();
    if (!IN_TAURI) {
      setSettings((s) => ({ ...s, user_id: id }));
      return;
    }
    void invoke("set_composio_user_id", { userId: id })
      .then(() => refreshSettings())
      .catch((e) => setErr(String(e)));
  };

  const saveKey = (): void => {
    const k = keyInput.trim();
    if (!k) return;
    if (!IN_TAURI) {
      setSettings((s) => ({ ...s, has_key: true, key_last4: k.slice(-4) }));
      setKeyInput("");
      return;
    }
    void invoke("set_composio_key", { key: k })
      .then(() => {
        setKeyInput("");
        refreshSettings();
      })
      .catch((e) => setErr(String(e)));
  };

  const removeKey = (): void => {
    if (!IN_TAURI) {
      setSettings((s) => ({ ...s, has_key: false, key_last4: "" }));
      return;
    }
    void invoke("clear_composio_key")
      .then(refreshSettings)
      .catch((e) => setErr(String(e)));
  };

  const grantConsent = (): void => {
    if (!IN_TAURI) {
      setSettings((s) => ({ ...s, consent_acknowledged: true }));
      return;
    }
    void invoke("set_composio_policy", {
      draftStop: settings.draft_stop,
      consentAcknowledged: true,
    })
      .then(refreshSettings)
      .catch((e) => setErr(String(e)));
  };

  const revokeConsent = (): void => {
    if (!IN_TAURI) {
      setSettings((s) => ({ ...s, consent_acknowledged: false, draft_stop: true }));
      return;
    }
    // Revoking forces draft_stop back ON (invariant: can't have live send without consent).
    void invoke("set_composio_policy", {
      draftStop: true,
      consentAcknowledged: false,
    })
      .then(() => {
        setCheck1(false);
        setCheck2(false);
        setCheck3(false);
        refreshSettings();
      })
      .catch((e) => setErr(String(e)));
  };

  const setDraftStop = (draftStop: boolean): void => {
    if (!IN_TAURI) {
      setSettings((s) => ({ ...s, draft_stop: draftStop }));
      return;
    }
    void invoke("set_composio_policy", {
      draftStop,
      consentAcknowledged: settings.consent_acknowledged,
    })
      .then(refreshSettings)
      .catch((e) => setErr(String(e)));
  };

  const allChecked = check1 && check2 && check3;

  return (
    <section className="set">
      <div className="set__label">{t.composioTitle}</div>
      <div className="set__hint">{t.composioHint}</div>
      {err ? <div className="set__hint is-err">{err}</div> : null}

      {/* API key row */}
      <div
        className={`set__hint${settings.has_key ? " is-ok" : ""}`}
      >
        {settings.has_key
          ? `${t.composioKeyPresent} ·· ${settings.key_last4}`
          : t.composioKeyAbsent}
      </div>
      <div className="keyrow">
        <input
          className="keyrow__input"
          type="password"
          placeholder={t.composioKeyPlaceholder}
          value={keyInput}
          autoComplete="off"
          onChange={(e) => setKeyInput(e.target.value)}
          onFocus={() => {
            if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") saveKey();
          }}
        />
        <button
          className="keyrow__btn"
          type="button"
          onClick={saveKey}
          disabled={!keyInput.trim()}
        >
          {t.keySave}
        </button>
        {settings.has_key ? (
          <button className="keyrow__btn" type="button" onClick={removeKey}>
            {t.keyRemove}
          </button>
        ) : null}
      </div>

      {/* User ID row */}
      <div className="set__label" style={{ marginTop: 10 }}>{t.composioUserId}</div>
      <div className="set__hint">{t.composioUserIdHint}</div>
      <div className="keyrow">
        <input
          className="keyrow__input"
          type="text"
          placeholder={t.composioUserId}
          value={userIdInput}
          autoComplete="off"
          onChange={(e) => setUserIdInput(e.target.value)}
          onFocus={() => {
            if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") saveUserId();
          }}
        />
        <button
          className="keyrow__btn"
          type="button"
          onClick={saveUserId}
        >
          {t.keySave}
        </button>
      </div>

      {/* Consent flow */}
      {settings.consent_acknowledged ? (
        <div className="keyrow" style={{ marginTop: 8 }}>
          <span className="set__hint is-ok">{t.composioConsentGranted}</span>
          <button className="keyrow__btn" type="button" onClick={revokeConsent}>
            {t.composioRevokeConsent}
          </button>
        </div>
      ) : (
        <div style={{ marginTop: 8 }}>
          <div className="set__hint">{t.composioConsentTitle}</div>
          <label className="set__hint" style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <input type="checkbox" checked={check1} onChange={(e) => setCheck1(e.target.checked)} />
            {t.composioConsentItem1}
          </label>
          <label className="set__hint" style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <input type="checkbox" checked={check2} onChange={(e) => setCheck2(e.target.checked)} />
            {t.composioConsentItem2}
          </label>
          <label className="set__hint" style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <input type="checkbox" checked={check3} onChange={(e) => setCheck3(e.target.checked)} />
            {t.composioConsentItem3}
          </label>
          <div className="keyrow" style={{ marginTop: 4 }}>
            <button
              className="keyrow__btn"
              type="button"
              disabled={!allChecked}
              onClick={grantConsent}
            >
              {t.composioGrantConsent}
            </button>
          </div>
        </div>
      )}

      {/* Draft-stop toggle — only operable once consent is granted */}
      <label
        className="set__hint"
        style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 8, opacity: settings.consent_acknowledged ? 1 : 0.4 }}
      >
        <input
          type="checkbox"
          checked={settings.draft_stop}
          disabled={!settings.consent_acknowledged}
          onChange={(e) => setDraftStop(e.target.checked)}
        />
        {t.composioDraftStop}
      </label>
    </section>
  );
}

// Castle Position (issue #20): the six resting places SHOGUN can live at, keyed by the wire form
// the Rust `castle` commands speak. The order is the reading order of the mini-screen diagram.
type CastlePos =
  | "notch"
  | "left_edge"
  | "right_edge"
  | "bottom_left"
  | "bottom_center"
  | "bottom_right";

const CASTLE_ANCHORS: Array<{ id: CastlePos; label: string; cls: string }> = [
  { id: "notch", label: t.castleNotch, cls: "castle__spot--notch" },
  { id: "left_edge", label: t.castleLeftEdge, cls: "castle__spot--left" },
  { id: "right_edge", label: t.castleRightEdge, cls: "castle__spot--right" },
  { id: "bottom_left", label: t.castleBottomLeft, cls: "castle__spot--bl" },
  { id: "bottom_center", label: t.castleBottomCenter, cls: "castle__spot--bc" },
  { id: "bottom_right", label: t.castleBottomRight, cls: "castle__spot--br" },
];

// Picker for where the panel resides on screen. A little Mac silhouette with the six anchor points
// drawn where they actually sit — the selected one glows. Choosing one re-docks the live panel
// immediately (the Rust side persists it and moves the window).
function CastlePositionSection(): JSX.Element {
  const [pos, setPos] = useState<CastlePos>("notch");

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<string>("get_castle_position")
      .then((p) => {
        if (CASTLE_ANCHORS.some((a) => a.id === p)) setPos(p as CastlePos);
      })
      .catch(() => undefined);
  }, []);

  const choose = (next: CastlePos): void => {
    if (next === pos) return;
    const prev = pos;
    setPos(next); // optimistic — the move should feel instant
    if (IN_TAURI)
      void invoke("set_castle_position", { position: next }).catch(() => setPos(prev));
  };

  return (
    <section className="set">
      <div className="set__label" id="seg-castle">{t.castle}</div>
      <div className="castle" role="radiogroup" aria-labelledby="seg-castle">
        <div className="castle__screen">
          {CASTLE_ANCHORS.map((a) => (
            <button
              key={a.id}
              type="button"
              role="radio"
              aria-checked={pos === a.id}
              aria-label={a.label}
              title={a.label}
              className={`castle__spot ${a.cls}${pos === a.id ? " is-on" : ""}`}
              onClick={() => choose(a.id)}
            />
          ))}
        </div>
      </div>
      <div className="set__hint">{t.castleHint}</div>
    </section>
  );
}

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
                <ServiceMark source={r.source} label={label} />
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
/** The model each provider runs. Mirrors default_model() in inline_source.rs — shown so the user
 *  can see what will run, not so they can change it. */
function defaultModelFor(provider: string): string {
  switch (provider) {
    case "openrouter":
      return "anthropic/claude-sonnet-4.5";
    case "openai":
      return "gpt-4o-mini";
    case "gemini":
      return "gemini-2.5-flash";
    default:
      return "claude-sonnet-5";
  }
}

const PROVIDERS: Array<{ id: string; label: string }> = [
  { id: "anthropic", label: "Claude API" },
  { id: "openrouter", label: "OpenRouter" },
  { id: "openai", label: "OpenAI" },
  { id: "gemini", label: "Gemini" },
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
  /// The provider refused this key — shown in the key section, since that is where the fix is.
  keyRejected: boolean;
  stateCount: number;
  onDone: () => void;
  onCleared: () => void;
}): JSX.Element {
  const { appearance, setAppearance, hasKey, keyRejected, stateCount, onDone, onCleared } = props;
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
        {/* Near the top on purpose. Meeting notes ships off and only ever turns on because
            someone found this switch — burying an opt-in below six connectors is how a feature
            stays permanently off (FR-MT-01). */}
        <MeetingSection />
        <VisualRecallSection />
        <ConnectionsSection />
        <ComposioSection />
        <AiSessionsSection />
        <DreamSection />
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
        <CastlePositionSection />
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
          {/* No free-text model field. It sat directly above the key entry and looked identical
              to it, so a pasted API key landed in the model — which was then sent as the model
              name and written to the log. The provider already has one right default; picking a
              model is not a decision this product needs to offer, and the field's only proven use
              was leaking a credential. */}
          <div className="set__hint">{t.modelFor} {defaultModelFor(provider)}</div>
          <div className="set__hint">{t.modelHint}</div>
        </section>
        <section className="set">
          <div className="set__label">{t.key}</div>
          <div
            className={`set__hint${keyRejected ? " is-err" : keyState ? " is-ok" : ""}`}
          >
            {keyRejected ? t.keyRejected : keyState ? t.keyPresent : t.keyAbsent}
          </div>
          <div className="set__hint">{t.keyScope}</div>
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
