import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { uiLog } from "./uiLog";
import {
  SummaryCard,
  SummaryMark,
  type DailySettings,
  type SummaryState as DailySummary,
  type SummaryWhich,
} from "./daily";

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

// Collapsed state: let the pill be dragged to a new spot (issue #21). The whole strip is one
// button whose click expands the panel, so we can't use beginDrag (it bails on buttons). Native
// performWindowDragWithEvent handles the distinction for us: a real drag moves the window and
// swallows the click, while a stationary press returns and falls through to onClick (expand) —
// so the pill stays click-to-open AND becomes drag-to-move without a manual threshold. Rust
// remembers the dropped spot as the resting place until a Castle Position is picked again
// (docs/fixes/2026-07-30-pill-drag-port-design.md).
function beginPillDrag(e: React.MouseEvent): void {
  if (!IN_TAURI || e.button !== 0) return;
  void invoke("start_panel_drag").catch((err) => uiLog(`start_panel_drag failed: ${err}`));
}
import { t, tf } from "./strings";
import { AnalyticsToggle } from "./AnalyticsToggle";
import { ConnectionsList } from "./connections";
import { comboChips, DEFAULT_BINDS } from "./keys";
import {
  IconClose,
  IconHistory,
  IconMaximize2,
  IconMinimize,
  IconPin,
  IconPinOff,
  IconSettings,
} from "./utilityIcons";
import {
  Activity as HubActivity,
  Memory as HubMemory,
  Sources as HubSources,
  Today as HubToday,
  Trace as HubTrace,
} from "./fullui/FullUi";
import { SAMPLE_VIEW } from "./fullui/sample";
import type { FullUiView } from "./fullui/types";

// SHOGUN panel. A visible, interactive window that hangs from the notch. Opening/closing is driven
// by direct clicks in the webview (reliable — no dependency on the CGEventTap hover path or a global
// hotkey), so it always works as long as the window renders. The Rust engine still feeds live
// `context` (the app you're reading) and the data lives in Rust (CLAUDE.md invariant 1); the webview
// owns only presentation. All-spaces/background float (NSPanel) is a separate, gated path.

type Appearance = "auto" | "light" | "dark";
type Citation = { event_id: number; source: string; title: string | null };
type Msg = { role: "me" | "shogun"; text: string; citations?: Citation[] };

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
  /** Capture/ASR paused; meeting session still open (waveform toggle). */
  paused?: boolean;
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

/** What the app could not set up at boot (startup_health.rs). Unlike InlineStatus these are not
 *  outcomes of an action — they are standing conditions, and they stay on screen until fixed. */
interface StartupHealth {
  /** Reason the memory DB could not be opened, or null when it opened normally. */
  memory_db_error: string | null;
  /** Read live on every query, so granting the permission clears the warning without a relaunch. */
  accessibility: boolean;
  /** false = hybrid search unavailable, results are lexical only. */
  embedding_model: boolean;
  /** The store opened but its last operation failed (issue #121). Self-clearing on the next
   *  successful read, so this is a live signal rather than a boot fact. */
  memory_degraded: boolean;
  /** Failure class when degraded ("lock_poisoned" / "query") — a tag, never a driver message. */
  memory_fault: string | null;
  /** Store failures since launch; monotonic, so repeated flapping stays visible. */
  memory_faults_total: number;
}

/** Hold-to-talk voice dialogue (#44). Rust owns lifecycle; notch expands on `voice_state`. */
export interface VoiceView {
  phase: "idle" | "recording" | "processing" | "response" | "error";
  transcript: string;
  response: string;
  error: string;
  level: number;
}

interface LevelEvent {
  rms: number;
}

const VOICE_W_RESPONSE = 480;
const VOICE_H_RESPONSE = 280;
const VOICE_W_RECORD_COLLAPSED = 240;
const VOICE_LEVEL_STALE_MS = 1_200;
const VOICE_ERROR_DISMISS_MS = 4_000;

function voicePanelSize(phase: VoiceView["phase"]): Size {
  switch (phase) {
    case "response":
      return { w: VOICE_W_RESPONSE, h: VOICE_H_RESPONSE };
    default:
      return { w: W, h: H_OPEN };
  }
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
/** Hardware notch cutout (safeAreaInsets.top). Welded black fills this — labels do not. */
const H_DEAD = 32;
/** Visible Idle content row below the cutout. Matches pre-weld 44px chin presence; text stays
 *  out of silicon (dead band above). Keep in sync with CSS `--chin-row-h` + Rust `IDLE_CONTENT_DROP`. */
const H_CHIN_ROW = 44;
/** Idle panel height = dead silicon band + readable content row. */
const H_HANDLE = H_DEAD + H_CHIN_ROW;
/** Leave grace after pointer exits unpinned panel (spec T4 / playbook P0: 300ms at R_exp). */
const AUTO_COLLAPSE_MS = 300;
/** Shell open paint budget — Idle→Expanded morph (CLAUDE.md ≤100ms). */
const OPEN_ANIM_MS = 100;
/** Shell close paint budget — keep panel mounted until scale finishes, then shrink frame. */
const COLLAPSE_ANIM_MS = 140;
/** How long the pill holds a ⌥-tap outcome before returning to the live source. Long enough to
 *  read, short enough that it never becomes something you have to dismiss. */
const INLINE_HOLD_MS = 2200;
/** ⌥-tap results the user has to act on. They persist instead of fading with INLINE_HOLD_MS. */
const STICKY_INLINE_PHASES = new Set<InlineStatus["phase"]>(["no_key", "key_rejected"]);
/** Ceiling on SILENCE within a chat turn, not on the turn itself. Answers stream, so "nothing has
 *  arrived for this long" is the honest test for a hung provider, where a flat ceiling on the whole
 *  turn cut off exactly the long grounded answers that were worth waiting for.
 *
 *  Left at the old whole-turn number rather than tightened. Against a streaming provider it is
 *  never reached — the first token lands in about a second and every delta rearms it — and the one
 *  path that still answers in a single delta (a subscription delegate, which is a vendor CLI
 *  subprocess with no incremental output) needs the full budget for its whole answer. Shortening it
 *  would only ever punish that path. Waiting is no longer the only option anyway: Stop is live for
 *  the whole turn. */
const CHAT_SILENCE_MS = 90_000;
const H_SETTINGS = 460; // taller default so setting groups fit; body scrolls; clamped to screen
const H_HUB = 560; // the in-panel hub draws the overview panes (cards, tables); give them room
const MIN_W = 460;
const MIN_H = 240;
// Collapsed floor + hard cap = hardware/pseudo notch width (~180). Wider Idle chrome (260–280)
// hung into empty menu-bar space and felt like an oversized hitbox outside the notch.
const W_HANDLE_FALLBACK = 180;
/** Quiet hiding Idle — hardware-notch-sized weld when frontmost is self / unknown. */
const W_HIDE = 180;

interface Size {
  w: number;
  h: number;
}

// Hard ceiling: panel cannot exceed ¾ of the display the webview is on. `window.screen` tracks the
// panel's monitor in Tauri; Rust `set_panel_size` enforces the same cap on the native frame.
const PANEL_MAX_SCREEN_FRAC = 0.75;
function maxSize(): Size {
  if (typeof window === "undefined") return { w: 1200, h: 800 };
  const sw = window.screen.width;
  const sh = window.screen.height;
  return {
    w: Math.max(MIN_W, Math.floor(sw * PANEL_MAX_SCREEN_FRAC)),
    h: Math.max(MIN_H, Math.floor(sh * PANEL_MAX_SCREEN_FRAC)),
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

/** One context-action button (B-1) — the `notch_actions` command's ActionView projection. */
interface ActionView {
  label: string;
  /** "L1" | "L2" | "L3" — the permission level the UI gates on (invariant 4 surfaced). */
  level: string;
  rationale: string;
}

/** One memory-search row (B-6) — the `search_memory` command's SearchHitView projection. */
interface SearchHitView {
  event_id: number;
  /** Unix ms — rendered as relative time. */
  ts: number;
  source: string;
  /** App bundle id when the event came from a window capture; empty otherwise. */
  app: string;
  excerpt: string;
}

const MOCK_ACTIONS: ActionView[] = [
  { label: "Search memory: Alice", level: "L1", rationale: "Overdue: send Alice the Q3 deck" },
  { label: "Draft vendor reply", level: "L2", rationale: "Reply to the vendor about pricing" },
];
const MOCK_HITS: SearchHitView[] = [
  { event_id: 1, ts: Date.now() - 40 * 60_000, source: "ax", app: "com.apple.mail", excerpt: "Alice asked for the Q3 deck by Friday — can you send the latest version?" },
  { event_id: 2, ts: Date.now() - 26 * 3_600_000, source: "ax", app: "com.tinyspeck.slackmacgap", excerpt: "vendor pricing thread: waiting on the updated quote before we reply" },
];

/** Compact relative time for search rows ("now", "5m", "3h", "2d"). */
function relTime(ts: number): string {
  const mins = Math.floor((Date.now() - ts) / 60_000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

/** The excerpt with each query match emphasized. Case-insensitive; falls back to plain text when
 *  lowercasing shifts offsets (rare locale-specific expansions), so a match is never mis-sliced. */
function emphasize(text: string, query: string): JSX.Element {
  const needle = query.trim().toLowerCase();
  const lower = text.toLowerCase();
  if (!needle || lower.length !== text.length) return <>{text}</>;
  const parts: JSX.Element[] = [];
  let from = 0;
  let key = 0;
  for (let at = lower.indexOf(needle); at >= 0; at = lower.indexOf(needle, from)) {
    if (at > from) parts.push(<span key={key++}>{text.slice(from, at)}</span>);
    parts.push(<b key={key++}>{text.slice(at, at + needle.length)}</b>);
    from = at + needle.length;
  }
  parts.push(<span key={key}>{text.slice(from)}</span>);
  return <>{parts}</>;
}

function appName(bundle: string): string {
  if (!bundle) return t.yourScreen;
  const seg = bundle.split(".").pop() || bundle;
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}

/** Own process / product labels — never show as Idle "reading …" (hiding chin instead). */
const OWN_FOCUS_IDS = new Set([
  "dev.shogun.spike",
  "shogunai",
  "shogun",
  "shogun-desktop-spike",
  "spike",
]);

function isSelfFocus(id: string): boolean {
  if (!id.trim()) return true; // unknown / cleared → quiet welded Idle
  const lower = id.trim().toLowerCase();
  if (OWN_FOCUS_IDS.has(lower)) return true;
  const seg = lower.split(".").pop() || lower;
  return OWN_FOCUS_IDS.has(seg);
}

/// How a resize should hold the panel in place.
/// - `center` (default): keep horizontal centre under the notch (castle dock). View switches and
///   the corner grip both use this — width changes ±dx/2 per side.
/// - `left`: legacy top-left pin (grow down/right only). Kept for the Rust command; unused by grip.
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
  /** Rust notch SM mirror — drives shell CSS only. Webview does not own dwell. */
  const [notchSm, setNotchSm] = useState<string>("expanded");
  /** True while open morph runs: full frame + idle-scale pose, then class flip to Expanded. */
  const [expanding, setExpanding] = useState(false);
  /** True while close scale runs; panel stays mounted until COLLAPSE_ANIM_MS elapses. */
  const [collapsing, setCollapsing] = useState(false);
  const collapseTimer = useRef<number | null>(null);
  const expandTimer = useRef<number | null>(null);
  const beginCollapseRef = useRef<() => void>(() => undefined);
  const expandRef = useRef<() => void>(() => undefined);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const openRef = useRef(true);
  const [status, setStatus] = useState<Status | null>(IN_TAURI ? null : MOCK_STATUS);
  const [state, setState] = useState<StateView>(IN_TAURI ? { commitments: [], open_loops: [] } : MOCK_STATE);
  // Daily summaries (issue #10): the delivery judgement, polled with the rest of the status
  // (the poll itself is the "user is here" activity signal), and the card currently open.
  // The open card keeps its own (which, date) so a dismissal after midnight still marks the
  // day it was delivered for.
  const [summary, setSummary] = useState<DailySummary | null>(null);
  const [summaryOpen, setSummaryOpen] = useState<{ which: SummaryWhich; date: string } | null>(null);
  const [ctxApp, setCtxApp] = useState<string>("");
  const [msgs, setMsgs] = useState<Msg[]>(() => loadJson<Msg[]>("shogun.msgs", []));
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  /// The answer being written right now, or null between turns. Held apart from `msgs` so a
  /// partial answer is never persisted and never mistaken for a finished one: it is committed
  /// into the thread on `chat_done` and nowhere else.
  const [streaming, setStreaming] = useState<string | null>(null);
  const [appearance, setAppearance] = useState<Appearance>(() => loadJson<Appearance>("shogun.appearance", "auto"));

  // Persist across window respawns (the Rust side rebuilds the window to change Spaces).
  // `open` is deliberately NOT persisted: launch is always expanded (see the useState above), so
  // writing it back would only be a value nothing reads.
  useEffect(() => saveJson("shogun.msgs", msgs.slice(-50)), [msgs]);
  useEffect(() => saveJson("shogun.appearance", appearance), [appearance]);
  const [showState, setShowState] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  /// In-panel memory search (B-6): toggled by the header button or the `/` shortcut.
  const [showSearch, setShowSearch] = useState(false);
  /// Pending L3 sends — the badge on the settings gear. Polled with the rest of the state and
  /// bumped optimistically when a context action queues one (the poll then corrects it).
  const [approvalsCount, setApprovalsCount] = useState(0);
  /// The ⌥-tap's own feedback. The pill shows it briefly and then returns to the live source —
  /// this is a reply to a keystroke, not a status the user has to dismiss.
  const [inline, setInline] = useState<InlineStatus | null>(null);
  const [health, setHealth] = useState<StartupHealth | null>(null);
  /// Pinned = the panel stays put. Unpinned = it withdraws as soon as the pointer leaves, which is
  /// the counterpart to opening on hover: the same gesture that summons it also dismisses it, so
  /// a glance costs no clicks at all. Persisted because the panel is respawned by Rust.
  const [pinned, setPinned] = useState<boolean>(() => loadJson<boolean>("shogun.pinned", true));
  /// Where this session's conversation begins in the persisted store. Captured once at mount, so
  /// anything above it is history rather than part of what you're doing now.
  const historyMark = useRef<number | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  /** Idle chin: reading/app/due vs quiet welded hide. Persisted in app data (Rust). */
  const [showStatusInNotch, setShowStatusInNotch] = useState(true);
  /// The in-panel hub (Today / Health / Sources / Memory / Activity / Trace). Everything routine
  /// finishes inside the notch — only meetings and Visual Recall get their own surfaces.
  const [showHub, setShowHub] = useState(false);
  const [voiceToast, setVoiceToast] = useState<string | null>(null);
  const [voice, setVoice] = useState<VoiceView>({
    phase: "idle",
    transcript: "",
    response: "",
    error: "",
    level: 0,
  });
  const voicePeak = useRef(0);
  /** Last `voice_level` timestamp — failsafe ends stuck recording after release + quiet. */
  const lastVoiceLevelAt = useRef(0);
  const voiceReleaseWatch = useRef<number | null>(null);
  const voiceMicWatch = useRef<number | null>(null);
  const voiceErrorDismiss = useRef<number | null>(null);
  // #120: these timers are OWNED — each new event clears its predecessor before arming, so a
  // stale timeout can never dismiss the state that replaced the one it was armed for.
  const inlineHideTimer = useRef<number | null>(null);
  const voiceToastTimer = useRef<number | null>(null);

  // Open-view size is user-resizable (corner grip) and persists across Rust-driven respawns.
  // Chat and Settings share one frame — toggling settings must not jump to a separate stored size.
  const [chatSize, setChatSize] = useState<Size>(() => {
    const s = loadJson<Size>("shogun.size.chat", { w: W, h: H_OPEN });
    return clampSize(s.w, s.h);
  });
  const [setSize, setSetSize] = useState<Size>(() => {
    const s = loadJson<Size>("shogun.size.settings", { w: W, h: H_SETTINGS });
    return clampSize(s.w, s.h);
  });
  const [hubSize, setHubSize] = useState<Size>(() => {
    const s = loadJson<Size>("shogun.size.hub", { w: W, h: H_HUB });
    return clampSize(s.w, s.h);
  });
  useEffect(() => saveJson("shogun.pinned", pinned), [pinned]);
  useEffect(() => saveJson("shogun.size.chat", chatSize), [chatSize]);
  useEffect(() => saveJson("shogun.size.settings", setSize), [setSize]);
  useEffect(() => saveJson("shogun.size.hub", hubSize), [hubSize]);

  // Size the window to match collapsed vs expanded. Pass explicit `open` when a state setter in the
  // same handler hasn't committed yet (React batches updates). Settings enter/exit does not resize.
  const sizeForView = useCallback(
    (opts?: { open?: boolean; settings?: boolean; hub?: boolean }): void => {
      const isOpen = opts?.open ?? open;
      const isSettings = opts?.settings ?? showSettings;
      const isHub = opts?.hub ?? showHub;
      // Collapsed: a provisional pill-sized window; the measuring effect below tightens it to the
      // pill's real bounds so the transparent remainder never eats clicks.
      if (!isOpen) void applyPanelSize(W_HANDLE_FALLBACK, H_HANDLE);
      else if (isSettings) void applyPanelSize(setSize.w, setSize.h);
      else if (isHub) void applyPanelSize(hubSize.w, hubSize.h);
      else void applyPanelSize(chatSize.w, chatSize.h);
    },
    [open, showSettings, showHub, chatSize, setSize, hubSize],
  );
  // The boot/summon listeners live in a run-once effect; a ref keeps them calling the LATEST sizer
  // instead of a stale closure captured at mount.
  const sizeForViewRef = useRef(sizeForView);
  sizeForViewRef.current = sizeForView;

  // Live resize from the corner grip. During the drag we only resize the native panel (the webview
  // reflows via CSS — no React state churn), rAF-throttled so we don't flood the IPC bridge. The
  // per-view size is committed to state (and persisted) once on release. Always castle-centre
  // anchor so width grows/shrinks symmetrically under the notch.
  const liveSize = useRef<Size | null>(null);
  const raf = useRef<number | null>(null);
  const onResizeLive = useCallback((w: number, h: number): void => {
    const s = clampSize(w, h);
    liveSize.current = s;
    if (raf.current == null) {
      raf.current = requestAnimationFrame(() => {
        raf.current = null;
        const cur = liveSize.current;
        if (cur) void applyPanelSize(cur.w, cur.h, "center");
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
    void applyPanelSize(s.w, s.h, "center");
    if (showSettings) setSetSize(s);
    else if (showHub) setHubSize(s);
    else setChatSize(s);
  }, [showSettings, showHub]);
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
  const meetingRef = useRef<MeetingView | null>(null);
  meetingRef.current = meeting;

  // Start at the open size and prove the webview is alive.
  useEffect(() => {
    if (!IN_TAURI) return;
    sizeForViewRef.current({ open: true });
    void invoke("interact", { kind: "boot" });
    const offs: Array<Promise<() => void>> = [];
    offs.push(listen<ContextPayload>("context", (e) => setCtxApp(e.payload.bundle_id || e.payload.title_masked || "")));
    // The pill is push-driven: Rust owns the lifecycle, the webview never decides that a meeting
    // has started (FR-MT-07). The first read covers a webview reload mid-meeting.
    offs.push(listen<MeetingView>("meeting", (e) => setMeeting(e.payload)));
    void invoke<MeetingView>("meeting_status").then(setMeeting).catch(() => undefined);
    // Boot conditions the app used to keep to itself (stderr only). Asked again on every expand
    // below, so granting Accessibility from System Settings clears the warning in place.
    void invoke<StartupHealth>("startup_health").then(setHealth).catch(() => undefined);
    offs.push(
      listen<InlineStatus>("inline", (e) => {
        setInline(e.payload);
        // A new phase supersedes whatever hide was pending — without this, the fade armed for a
        // finished draft fires into the NEXT draft's `drafting` spinner and blanks it (#120).
        if (inlineHideTimer.current != null) {
          window.clearTimeout(inlineHideTimer.current);
          inlineHideTimer.current = null;
        }
        // `drafting` holds until the outcome replaces it — a spinner that timed itself out would
        // claim the draft had finished when it hadn't. A missing or rejected key is not an outcome
        // either: nothing clears it but a trip to settings, and timing it out after two seconds is
        // how a 401 came to look identical to a shortcut that simply does not fire. Those stay up
        // until the next tap replaces them.
        if (e.payload.phase !== "drafting" && !STICKY_INLINE_PHASES.has(e.payload.phase)) {
          inlineHideTimer.current = window.setTimeout(() => {
            inlineHideTimer.current = null;
            setInline(null);
          }, INLINE_HOLD_MS);
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
    offs.push(
      listen<{ phase: VoiceView["phase"]; transcript?: string | null; response?: string | null }>(
        "voice_state",
        (e) => {
          const p = e.payload.phase;
          if (p !== "error" && voiceErrorDismiss.current != null) {
            window.clearTimeout(voiceErrorDismiss.current);
            voiceErrorDismiss.current = null;
          }
          setVoice((cur) => ({
            ...cur,
            phase: p,
            transcript: e.payload.transcript ?? (p === "idle" ? "" : cur.transcript),
            response: e.payload.response ?? (p === "idle" ? "" : cur.response),
            error: p === "error" ? e.payload.response ?? cur.error : p === "idle" ? "" : cur.error,
            level: p === "recording" ? cur.level : 0,
          }));
          if (p !== "recording" && voiceMicWatch.current != null) {
            window.clearInterval(voiceMicWatch.current);
            voiceMicWatch.current = null;
          }
          if (p === "recording") {
            voicePeak.current = 0;
            lastVoiceLevelAt.current = performance.now();
            if (voiceMicWatch.current != null) window.clearInterval(voiceMicWatch.current);
            // The audio lane emits a frame even for silence. If frames stop, close this session
            // so a live indicator cannot claim capture is still running.
            voiceMicWatch.current = window.setInterval(() => {
              if (voiceRef.current.phase !== "recording") return;
              if (performance.now() - lastVoiceLevelAt.current < VOICE_LEVEL_STALE_MS) return;
              if (voiceMicWatch.current != null) {
                window.clearInterval(voiceMicWatch.current);
                voiceMicWatch.current = null;
              }
              void invoke("voice_force_end").catch(() => undefined);
            }, 250);
            if (collapseTimer.current != null) {
              window.clearTimeout(collapseTimer.current);
              collapseTimer.current = null;
            }
            if (expandTimer.current != null) {
              window.clearTimeout(expandTimer.current);
              expandTimer.current = null;
            }
            setOpen(false);
            setExpanding(false);
            setCollapsing(false);
            setNotchSm("idle");
            void applyPanelSize(VOICE_W_RECORD_COLLAPSED, H_DEAD);
          } else if (p === "processing") {
            if (collapseTimer.current != null) {
              window.clearTimeout(collapseTimer.current);
              collapseTimer.current = null;
            }
            if (expandTimer.current != null) {
              window.clearTimeout(expandTimer.current);
              expandTimer.current = null;
            }
            setShowSettings(false);
            setOpen(false);
            setExpanding(false);
            setCollapsing(false);
            setNotchSm("idle");
            void applyPanelSize(VOICE_W_RECORD_COLLAPSED, H_DEAD);
          } else if (p === "idle") {
            // Dictation done — always collapse; do not leave recording chrome stuck open.
            beginCollapseRef.current();
          } else if (p === "error") {
            setOpen(true);
            setShowSettings(false);
            const sz = voicePanelSize(p);
            void applyPanelSize(sz.w, sz.h);
            if (voiceErrorDismiss.current != null) window.clearTimeout(voiceErrorDismiss.current);
            voiceErrorDismiss.current = window.setTimeout(() => {
              voiceErrorDismiss.current = null;
              if (voiceRef.current.phase !== "error") return;
              void invoke("voice_dismiss").catch(() => {
                if (voiceRef.current.phase !== "error") return;
                setVoice((current) =>
                  current.phase === "error"
                    ? { ...current, phase: "idle", transcript: "", response: "", error: "", level: 0 }
                    : current,
                );
                beginCollapseRef.current();
              });
            }, VOICE_ERROR_DISMISS_MS);
          } else if (p === "response") {
            setOpen(true);
            setShowSettings(false);
            const sz = voicePanelSize(p);
            void applyPanelSize(sz.w, sz.h);
          }
        },
      ),
    );
    offs.push(
      listen<LevelEvent>("voice_level", (e) => {
        const rms = e.payload.rms;
        voicePeak.current = Math.max(voicePeak.current * 0.85, rms);
        const norm = voicePeak.current > 0 ? Math.min(1, rms / voicePeak.current) : 0;
        lastVoiceLevelAt.current = performance.now();
        setVoice((cur) => (cur.phase === "recording" ? { ...cur, level: norm } : cur));
      }),
    );
    offs.push(
      listen<{ message: string }>("voice_toast", (e) => {
        setVoiceToast(e.payload.message);
        // Owned + superseding (#120): a short toast armed earlier must not dismiss a longer
        // error that replaced it.
        if (voiceToastTimer.current != null) window.clearTimeout(voiceToastTimer.current);
        voiceToastTimer.current = window.setTimeout(() => {
          voiceToastTimer.current = null;
          setVoiceToast(null);
        }, 2200);
      }),
    );
    // Release signal from Rust: if UI still shows recording after 500ms with no levels, force end.
    offs.push(
      listen("voice_hold_released", () => {
        if (voiceReleaseWatch.current != null) {
          window.clearInterval(voiceReleaseWatch.current);
          voiceReleaseWatch.current = null;
        }
        const started = performance.now();
        voiceReleaseWatch.current = window.setInterval(() => {
          const phase = voiceRef.current.phase;
          if (phase !== "recording") {
            if (voiceReleaseWatch.current != null) {
              window.clearInterval(voiceReleaseWatch.current);
              voiceReleaseWatch.current = null;
            }
            return;
          }
          const quietMs = performance.now() - lastVoiceLevelAt.current;
          const waited = performance.now() - started;
          // Absolute lifetime (#120): the release already happened, so the recording MUST end.
          // Waiting for 500ms of quiet is a nicety for trailing audio — a level stream that never
          // goes quiet would otherwise keep this interval alive forever with the UI stuck on
          // "recording". After 8s, end it regardless.
          if ((waited >= 500 && quietMs >= 500) || waited >= 8_000) {
            if (voiceReleaseWatch.current != null) {
              window.clearInterval(voiceReleaseWatch.current);
              voiceReleaseWatch.current = null;
            }
            void invoke("voice_force_end").catch(() => undefined);
          }
        }, 100);
      }),
    );
    // Rust owns hover intent / dwell. Webview only paints shell classes + open/close.
    offs.push(
      listen<StatePayload>("state", (e) => {
        const st = e.payload.state;
        setNotchSm(st);
        if (st === "hover" || st === "expanded") {
          // Q2 / SLO-01: report paint completion for this transition. Double-rAF so the panel is
          // actually on screen; Rust pairs it with the tracker's commit timestamp and drops
          // anything unpaired. Without this call the expand-latency SLO records nothing at all.
          //
          // Only when the panel was actually closed: an `expanded` event that arrives while it is
          // already open repaints nothing, and timing that would report a transition that never
          // happened (Rust drops the unpaired sample, but the honest thing is not to send it).
          if (!openRef.current) {
            requestAnimationFrame(() =>
              requestAnimationFrame(() =>
                void invoke("painted", { state: st, t1PerfMs: performance.now() }).catch(() => undefined),
              ),
            );
          }
          // Never fight a user who pinned the panel open, and never re-open one they just closed
          // by hand — the tracker doesn't know about either.
          if (!openRef.current) {
            // Meeting chin owns Stop / Take Notes. expand() sets expanding/open and unmounts
            // MeetingPill (showIdleFace=false) mid-click — hover looked like it worked, Stop never fired.
            // Hover only: Expanded is a deliberate open (hotkey, real click), and swallowing it
            // here left the panel unreachable for the whole meeting.
            const m = meetingRef.current;
            if (st === "hover" && m?.enabled && (m.state === "offered" || m.state === "recording")) {
              return;
            }
            expandRef.current();
          }
        } else if (st === "idle" || st === "hidden" || st === "collapsing") {
          // Withdraw on the same rule as the pointer-leave path: pinned stays, work in progress
          // stays, everything else follows your attention.
          if (pinnedRef.current) return;
          if (inputRef.current.trim().length > 0 || thinkingRef.current) return;
          if (voiceRef.current.phase === "recording" || voiceRef.current.phase === "processing") return;
          beginCollapseRef.current();
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
      if (e.key !== "Escape") return;
      // …but while an answer is arriving, Escape means "stop this", not "throw the panel away".
      // Hiding mid-answer would abandon text the user is reading with no way back to it.
      // (`stopRef`/`liveTurn` are refs, so this once-registered listener still sees the current
      // turn rather than the one that existed at mount.)
      if (liveTurn.current != null) {
        stopRef.current?.();
        return;
      }
      // Tell the tracker WHY the panel is going away — otherwise every session's close reason
      // reads as a timeout and the Q4 false-positive tally counts real uses as misfires.
      void invoke("collapse_request", { reason: "esc" }).catch(() => undefined);
      void invoke("hide_panel").catch(() => undefined);
    };
    window.addEventListener("keydown", onEsc);
    // Q4: count real interactions (and reset the Expanded idle timer). Deliberately coarse —
    // kinds only, never content.
    const onClick = (): void => void invoke("interact", { kind: "click" }).catch(() => undefined);
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== "Escape") void invoke("interact", { kind: "key" }).catch(() => undefined);
    };
    const onScroll = (): void => void invoke("interact", { kind: "scroll" }).catch(() => undefined);
    window.addEventListener("pointerdown", onClick);
    window.addEventListener("keydown", onKey);
    window.addEventListener("wheel", onScroll, { passive: true });
    return () => {
      window.removeEventListener("keydown", onEsc);
      if (voiceReleaseWatch.current != null) {
        window.clearInterval(voiceReleaseWatch.current);
        voiceReleaseWatch.current = null;
      }
      if (voiceMicWatch.current != null) {
        window.clearInterval(voiceMicWatch.current);
        voiceMicWatch.current = null;
      }
      if (voiceErrorDismiss.current != null) {
        window.clearTimeout(voiceErrorDismiss.current);
        voiceErrorDismiss.current = null;
      }
      if (inlineHideTimer.current != null) {
        window.clearTimeout(inlineHideTimer.current);
        inlineHideTimer.current = null;
      }
      if (voiceToastTimer.current != null) {
        window.clearTimeout(voiceToastTimer.current);
        voiceToastTimer.current = null;
      }
      offs.forEach((p) => void p.then((off) => off()));
    };
  }, []);

  const refreshState = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<Status>("shogun_status").then((s) => setStatus(s)).catch(() => undefined);
    void invoke<StateView>("shogun_state").then((s) => s && setState(s)).catch(() => undefined);
    // Count only — the rows themselves are the ApprovalsSection's business (Settings).
    void invoke<unknown[]>("list_approvals")
      .then((r) => setApprovalsCount(Array.isArray(r) ? r.length : 0))
      .catch(() => undefined);
    void invoke<boolean>("get_notch_status_visible")
      .then(setShowStatusInNotch)
      .catch(() => undefined);
    // The delivery judgement (issue #10). Each poll is a live "user is here" moment — the Rust
    // side decides morning/evening/nothing and cues at most once per delivered card.
    void invoke<DailySummary>("summary_state")
      .then(setSummary)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!IN_TAURI) return;
    refreshState();
    const id = setInterval(refreshState, 3000);
    return () => clearInterval(id);
  }, [refreshState]);

  // The notch opens onto a due summary (issue #10): only on the transition into open, so a
  // dismissed card never re-hijacks a panel that is already in use. Refs for the judgement —
  // this must fire on the open edge, not re-run on every poll.
  const summaryRef = useRef(summary);
  summaryRef.current = summary;
  const wasOpenForSummary = useRef(false);
  useEffect(() => {
    const s = summaryRef.current;
    if (open && !wasOpenForSummary.current && s?.due && !showSettings && !showHub) {
      setSummaryOpen({ which: s.due, date: s.date });
    }
    wasOpenForSummary.current = open;
  }, [open, showSettings, showHub]);

  useEffect(() => {
    if (!IN_TAURI) return;
    let unlisten: (() => void) | undefined;
    void listen<{ message: string }>("voice_error", (e) => {
      setVoiceToast(e.payload.message);
      if (voiceToastTimer.current != null) window.clearTimeout(voiceToastTimer.current);
      voiceToastTimer.current = window.setTimeout(() => {
        voiceToastTimer.current = null;
        setVoiceToast(null);
      }, 4000);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  // `/` opens the memory search (B-6) — a reach-anywhere shortcut, but never while typing:
  // a slash in the composer (or any field) is text, not a command.
  useEffect(() => {
    const onSlash = (e: KeyboardEvent): void => {
      if (e.key !== "/" || e.metaKey || e.ctrlKey || e.altKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (!open || showSettings || showHub) return;
      e.preventDefault();
      setShowSearch(true);
    };
    window.addEventListener("keydown", onSlash);
    return () => window.removeEventListener("keydown", onSlash);
  }, [open, showSettings, showHub]);

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
    // `auto` while text is streaming: a smooth scroll restarts on every token and never catches
    // up, so the newest line ends up permanently just below the fold.
    threadRef.current?.scrollTo({
      top: threadRef.current.scrollHeight,
      behavior: streaming == null ? "smooth" : "auto",
    });
  }, [msgs, thinking, streaming, open]);

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
  const voiceRef = useRef(voice);
  voiceRef.current = voice;
  const voiceActive = voice.phase !== "idle";

  const onPanelLeave = useCallback((): void => {
    if (pinned) return;
    cancelAutoCollapse();
    leaveTimer.current = window.setTimeout(() => {
      leaveTimer.current = null;
      // Never collapse over work in progress: a focused composer, a half-written question, or an
      // answer still arriving all mean the panel is in use even though the cursor wandered off.
      const composerHasFocus = document.activeElement?.classList.contains("composer__input") ?? false;
      if (composerHasFocus || inputRef.current.trim().length > 0 || thinkingRef.current) return;
      if (voiceRef.current.phase === "recording" || voiceRef.current.phase === "processing") return;
      beginCollapseRef.current();
    }, AUTO_COLLAPSE_MS);
  }, [pinned, cancelAutoCollapse]);
  useEffect(() => cancelAutoCollapse, [cancelAutoCollapse]);

  openRef.current = open;
  /// Whether this expand's SLO-02 sample has already been taken (see `claimActionsSlo`).
  const actionsSloClaimed = useRef(false);

  /** Paint chin→panel scale factors on the stage (compositor-only; no React re-render). */
  const applyMorphScale = useCallback((w: number, h: number): void => {
    const el = stageRef.current;
    if (!el) return;
    const chinW = handleRef.current?.getBoundingClientRect().width
      ?? pillRef.current?.getBoundingClientRect().width
      ?? W_HANDLE_FALLBACK;
    const sx = Math.min(1, Math.max(0.22, chinW / Math.max(1, w)));
    const sy = Math.min(1, Math.max(0.06, H_HANDLE / Math.max(1, h)));
    el.style.setProperty("--shell-sx", sx.toFixed(4));
    el.style.setProperty("--shell-sy", sy.toFixed(4));
  }, []);

  const beginCollapse = useCallback((): void => {
    if (collapseTimer.current != null) return;
    if (expandTimer.current != null) {
      window.clearTimeout(expandTimer.current);
      expandTimer.current = null;
    }
    setExpanding(false);
    if (!openRef.current) {
      setCollapsing(false);
      setNotchSm("idle");
      return;
    }
    // Morph from the size the panel is ACTUALLY at (settings/hub have their own), and clear the
    // view flags here so every collapse path — pointer-leave, Rust idle/hidden, voice — converges
    // on the same reset; a stale showHub would otherwise re-open the hub at chat size (see
    // expand()).
    const cur = showSettings ? setSize : showHub ? hubSize : chatSize;
    setShowSettings(false);
    setShowHub(false);
    // A collapsed card is done: it was marked seen on open, so the next expand goes back to
    // chat rather than re-presenting a summary the user already walked away from.
    setSummaryOpen(null);
    applyMorphScale(cur.w, cur.h);
    setCollapsing(true);
    setNotchSm((s) => (s === "idle" || s === "hidden" ? s : "collapsing"));
    collapseTimer.current = window.setTimeout(() => {
      collapseTimer.current = null;
      setCollapsing(false);
      setOpen(false);
      setNotchSm("idle");
      sizeForViewRef.current({ open: false });
    }, COLLAPSE_ANIM_MS);
  }, [applyMorphScale, chatSize, setSize, hubSize, showSettings, showHub]);
  beginCollapseRef.current = beginCollapse;

  const collapse = (): void => {
    beginCollapse();
  };
  const expand = useCallback((): void => {
    // A new expand is a new SLO-02 measurement window (see `claimActionsSlo`).
    actionsSloClaimed.current = false;
    // Accessibility is the one condition the user can fix without restarting, so re-read rather
    // than trust the value from boot.
    void invoke<StartupHealth>("startup_health").then(setHealth).catch(() => undefined);
    if (collapseTimer.current != null) {
      window.clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
    if (expandTimer.current != null) {
      window.clearTimeout(expandTimer.current);
      expandTimer.current = null;
    }
    setCollapsing(false);
    setNotchSm("expanded");
    // The view flags survive a deliberate open (hotkey while settings were up), so open at THAT
    // view's size — a chat-sized settings panel clips its groups.
    const cur = showSettings ? setSize : showHub ? hubSize : chatSize;
    applyMorphScale(cur.w, cur.h);
    // 1) Grow NSPanel to full frame. 2) Pose visible shell at Idle scale. 3) Flip to Expanded
    // so transform actually transitions (resize+class same tick kills the morph).
    void (async () => {
      if (IN_TAURI) await applyPanelSize(cur.w, cur.h);
      // Commit Idle-scale pose synchronously so the next frame only retargets transform.
      flushSync(() => {
        setExpanding(true);
      });
      const panel = stageRef.current?.querySelector(".panel");
      if (panel instanceof HTMLElement) void panel.offsetHeight;
      window.requestAnimationFrame(() => {
        setOpen(true);
        expandTimer.current = window.setTimeout(() => {
          expandTimer.current = null;
          setExpanding(false);
        }, OPEN_ANIM_MS);
      });
    })();
  }, [applyMorphScale, chatSize, setSize, hubSize, showSettings, showHub]);
  expandRef.current = expand;

  useEffect(
    () => () => {
      if (collapseTimer.current != null) window.clearTimeout(collapseTimer.current);
      if (expandTimer.current != null) window.clearTimeout(expandTimer.current);
    },
    [],
  );
  /// SLO-02 is "expand → context buttons painted", but `ActionsRow` also remounts when the Hub
  /// is toggled, and each mount would otherwise record a fresh sample anchored to the Hub close
  /// — percentiles polluted by a measurement of something else entirely. The row may claim the
  /// slot once per expand; later mounts refetch their actions but stay out of the numbers.
  const claimActionsSlo = useCallback((): boolean => {
    if (actionsSloClaimed.current) return false;
    actionsSloClaimed.current = true;
    return true;
  }, []);

  const openSettings = (): void => {
    setShowSettings(true);
    setShowHub(false);
    sizeForView({ open: true, settings: true, hub: false });
  };
  const closeSettings = (): void => {
    setShowSettings(false);
    sizeForView({ open: true, settings: false, hub: false });
  };
  const toggleHub = (): void => {
    const next = !showHub;
    setShowHub(next);
    setShowSettings(false);
    sizeForView({ open: true, settings: false, hub: next });
  };

  // ---- streamed chat turn ------------------------------------------------------------------
  // Refs rather than state for everything the event listeners consult: the listeners are
  // registered once at mount, so a captured value would be frozen at the first turn forever.

  /// The turn ids we hand to Rust. Client-minted so a delta can never arrive for an id the panel
  /// doesn't recognise yet.
  const turnSeq = useRef(0);
  /// The turn currently being written, or null. Deltas for any other id are stale — a stopped or
  /// timed-out turn whose provider hadn't noticed yet — and are dropped.
  const liveTurn = useRef<number | null>(null);
  /// What has arrived so far for `liveTurn`. Mirrors the `streaming` state so the listeners and
  /// timers can read "now" without re-registering on every token.
  const partialRef = useRef("");
  const watchdog = useRef<number | null>(null);
  /// So the once-registered Escape handler can reach the current `stopStreaming`.
  const stopRef = useRef<(() => void) | null>(null);
  /// Pending repaint. Deltas arrive far faster than the screen refreshes — a fast provider sends
  /// tokens in bursts — so the text is accumulated in `partialRef` and pushed to state once per
  /// frame. Rendering per token would spend more time in React than the tokens save.
  const paintFrame = useRef<number | null>(null);

  const paintPartial = useCallback((): void => {
    if (paintFrame.current != null) return;
    paintFrame.current = window.requestAnimationFrame(() => {
      paintFrame.current = null;
      setStreaming(partialRef.current);
    });
  }, []);

  const cancelPaint = useCallback((): void => {
    if (paintFrame.current != null) {
      window.cancelAnimationFrame(paintFrame.current);
      paintFrame.current = null;
    }
  }, []);

  /// Close the turn: commit whatever arrived (plus `suffix`, when the turn didn't end on its own
  /// terms) into the thread. Partial text is kept rather than thrown away — the tokens that did
  /// arrive are as grounded as the ones that didn't, and replacing them with an error loses work
  /// the user was already reading.
  const endTurn = useCallback(
    (suffix?: string, citations?: Citation[]): void => {
      liveTurn.current = null;
      if (watchdog.current != null) {
        window.clearTimeout(watchdog.current);
        watchdog.current = null;
      }
      // A queued repaint would land after the answer has already been committed to the thread,
      // and would put the partial back on screen underneath it.
      cancelPaint();
      const partial = partialRef.current.trim();
      partialRef.current = "";
      const text = suffix ? (partial ? `${partial}\n\n${suffix}` : suffix) : partial || t.noAnswer;
      setMsgs((m) => [...m, { role: "shogun", text, citations }]);
      setStreaming(null);
      setThinking(false);
    },
    [cancelPaint],
  );

  const armWatchdog = useCallback(
    (turn: number): void => {
      if (watchdog.current != null) window.clearTimeout(watchdog.current);
      watchdog.current = window.setTimeout(() => {
        if (liveTurn.current !== turn) return;
        if (IN_TAURI) void invoke("shogun_chat_cancel", { turn }).catch(() => undefined);
        endTurn(`${t.answerFailed}: timed out`);
      }, CHAT_SILENCE_MS);
    },
    [endTurn],
  );

  // The answer as it is written. Registered once; everything it needs is behind a ref.
  useEffect(() => {
    if (!IN_TAURI) return;
    const offs: Array<Promise<() => void>> = [
      listen<{ turn: number; text: string }>("chat_delta", (e) => {
        if (liveTurn.current !== e.payload.turn) return;
        armWatchdog(e.payload.turn);
        partialRef.current += e.payload.text;
        paintPartial();
      }),
      listen<{ turn: number; citations: Citation[]; error: string | null }>("chat_done", (e) => {
        if (liveTurn.current !== e.payload.turn) return;
        const { error, citations } = e.payload;
        endTurn(error ? `${t.answerFailed}: ${error}` : undefined, citations);
      }),
    ];
    return () => offs.forEach((p) => void p.then((off) => off()));
  }, [armWatchdog, endTurn, paintPartial]);

  // A turn in flight must not outlive the component: a fired watchdog or a queued repaint would
  // both call setState on something that is gone.
  useEffect(
    () => () => {
      cancelPaint();
      if (watchdog.current != null) window.clearTimeout(watchdog.current);
    },
    [cancelPaint],
  );

  const send = useCallback((): void => {
    const q = input.trim();
    if (!q || thinking) return;
    setInput("");
    setMsgs((m) => [...m, { role: "me", text: q }]);
    setThinking(true);
    setStreaming("");
    const finish = (text: string, citations?: Citation[]): void => {
      setMsgs((m) => [...m, { role: "shogun", text, citations }]);
      setStreaming(null);
      setThinking(false);
    };
    // No key means the backend would answer from the echo mock, so say so directly rather than
    // round-tripping for a non-answer. Resolves in this tick, so no turn is ever in flight.
    if (IN_TAURI && status && !status.has_key) {
      finish(t.noKey);
      return;
    }

    // The turn id is minted here, not by Rust: the first token can beat the command's own reply,
    // and a panel that doesn't yet know the id would have to buffer deltas it can't place.
    const turn = ++turnSeq.current;
    liveTurn.current = turn;

    partialRef.current = "";

    // Browser mock (`pnpm dev:vite`, no backend). Routed through the same turn lifecycle rather
    // than its own bare setTimeout: otherwise Stop renders for the delay and does nothing, which
    // is the one behaviour a UI mock exists to get right.
    if (!IN_TAURI) {
      watchdog.current = window.setTimeout(() => {
        if (liveTurn.current !== turn) return;
        partialRef.current = "I'd start with the overdue deck for Alice — want me to draft it at your cursor?";
        endTurn();
      }, 700);
      return;
    }

    // The ceiling is on SILENCE, not on the answer. A stream still producing tokens is not a hung
    // provider, and a flat ceiling on the whole turn punished exactly the long grounded answers
    // that are worth waiting for. Every delta rearms it (see the chat_delta listener).
    armWatchdog(turn);

    void invoke("shogun_chat_stream", { message: q, turn }).catch((e) => {
      if (liveTurn.current !== turn) return;
      endTurn(`${t.answerFailed}: ${e}`);
    });
  }, [input, thinking, status, armWatchdog, endTurn]);

  /// Stop the turn in flight. Rust abandons the HTTP response rather than reading it to the end,
  /// so stopping actually stops the work — it isn't just the UI looking away.
  const stopStreaming = useCallback((): void => {
    const turn = liveTurn.current;
    if (turn == null) return;
    if (IN_TAURI) void invoke("shogun_chat_cancel", { turn }).catch(() => undefined);
    endTurn(t.answerStopped);
  }, [endTurn]);
  stopRef.current = stopStreaming;

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

  // A standing condition from boot, if any. Ordered by how much of the product it takes away:
  // no memory is everything, no Accessibility is every input path, no model is only the search
  // quality. Only the worst one is shown — a stack of warnings in a notch panel reads as noise.
  const healthLine = ((): { text: string; fix: "settings" | "accessibility" | null } | null => {
    if (!health) return null;
    if (health.memory_db_error) return { text: t.healthNoMemory, fix: "settings" };
    // The store opened but is not answering (issue #121). Ranked right under "no memory at all"
    // because the consequence is the same — what SHOGUN knows is unreadable — and silence here
    // is what makes an empty answer look like an honest one.
    if (health.memory_degraded) return { text: t.healthMemoryDegraded, fix: null };
    if (!health.accessibility) return { text: t.healthNoAccess, fix: "accessibility" };
    if (!health.embedding_model) return { text: t.healthNoModel, fix: null };
    return null;
  })();

  // The ⌥-tap result wins the slot while it is showing: it answers something the user just did.
  const noticeLine = inlineLine
    ? { text: inlineLine.text, tone: inlineLine.tone, fix: null as "settings" | "accessibility" | null }
    : healthLine
      ? { text: healthLine.text, tone: "warn" as const, fix: healthLine.fix }
      : null;

  // A due summary greets from the collapsed handle (issue #10): the confirmed mark plus
  // "Good morning" and a soft brand glow — presence, not alarm. Warnings and ⌥-tap results
  // outrank it; the greeting keeps until the card is opened or the day moves on.
  const summaryDue = summary?.due ?? null;

  const runFix = (fix: "settings" | "accessibility" | null): void => {
    if (fix === "accessibility") {
      void invoke("open_accessibility_settings")
        // Re-read straight away: the user may grant it while the pane is open.
        .then(() => invoke<StartupHealth>("startup_health").then(setHealth))
        .catch(() => undefined);
      return;
    }
    // In-panel Settings, not the standalone Full UI window: the overview moved into the notch
    // (the Hub), and a health chip that spawned a separate window contradicted that decision —
    // the fix for every "settings" health line lives one click away, right here.
    if (fix === "settings") openSettings();
  };

  const totalState = state.commitments.length + state.open_loops.length;
  const focusId = ctxApp || status?.app || "";
  const selfFocus = isSelfFocus(focusId);
  const live = selfFocus ? "" : appName(focusId);
  const providerLabel = PROVIDERS.find((p) => p.id === provider)?.label ?? t.model;
  // OFF = always welded hide (stricter than self-focus). ON = self-focus hides unless inline.
  const hideIdleChin = !showStatusInNotch || (selfFocus && !inlineLine);

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
    // Cap Idle at notch width — never grow into empty menu-bar left/right of the cutout.
    // Hiding Idle uses the same weld (W_HIDE × H_DEAD).
    // Height floors at H_HANDLE so a short content pill never leaves air under the notch.
    const hiding = el.classList.contains("handle--hiding");
    const voiceActivity = el.querySelector(".vpill") !== null;
    const notchW = voiceActivity ? VOICE_W_RECORD_COLLAPSED : hiding ? W_HIDE : W_HANDLE_FALLBACK;
    const minH = voiceActivity || hiding ? H_DEAD : H_HANDLE;
    void applyPanelSize(
      notchW,
      Math.max(minH, Math.ceil(r.height)),
    );
  }, [
    open,
    live,
    selfFocus,
    showStatusInNotch,
    hideIdleChin,
    state.commitments.length,
    state.open_loops.length,
    meeting?.state,
    meeting?.title,
    meeting?.elapsed_ms,
    meeting?.countdown_ms,
    voice?.phase,
    voice?.level,
    // The greeting swaps the handle's text (issue #10), so its width changes with it.
    summary?.due,
  ]);

  const meetingLive =
    meeting?.enabled && (meeting.state === "offered" || meeting.state === "recording")
      ? meeting
      : null;

  const voiceLive = voiceActive ? voice : null;

  // Shell class: Rust SM when available; expanding/collapsing override for morph paint.
  // Rust emits "hover" (HoverIntent); map to is-hoverintent for CSS.
  const shellMode = collapsing
    ? "is-collapsing"
    : expanding && !open
      ? "is-expanding"
      : open
        ? "is-expanded"
        : notchSm === "hover" || notchSm === "hoverintent"
          ? "is-hoverintent"
          : "is-idle";

  // Idle face only when fully collapsed — never during expand/collapse (double-pill flash).
  const showIdleFace = !open && !collapsing && !expanding;

  const idleChin = !showIdleFace ? null : meetingLive ? (
    <div ref={pillRef}>
      <MeetingPill view={meetingLive} />
    </div>
  ) : voiceLive?.phase === "recording" ? (
    <div ref={pillRef}>
      <VoicePill />
    </div>
  ) : voiceLive?.phase === "processing" ? (
    <div ref={pillRef}>
      <VoiceProcessingPill />
    </div>
  ) : (
    <button
      className={`handle${hideIdleChin ? " handle--hiding" : ""}${
        !hideIdleChin && !noticeLine && summaryDue ? " handle--summary" : ""
      }`}
      ref={handleRef}
      type="button"
      onClick={() => expand()}
      onMouseDown={beginPillDrag}
      title={t.openPanel}
      aria-label={t.openPanel}
    >
      <span className="handle__dead" aria-hidden />
      {hideIdleChin ? null : (
        <span className="handle__row">
          {noticeLine ? (
            <span className={`handle__live inline--${noticeLine.tone}`}>
              <span className={`inline__dot inline__dot--${noticeLine.tone}`} />
              {noticeLine.text}
            </span>
          ) : summaryDue ? (
            <span className="handle__live handle__greet">
              <SummaryMark className="handle__greetmark" />
              {summaryDue === "morning" ? t.goodMorning : t.goodEvening}
            </span>
          ) : (
            <span className="handle__live">
              <span className="live__dot" />
              {t.reading} <b>{live}</b>
            </span>
          )}
        </span>
      )}
    </button>
  );

  // Panel always mounted (playbook P0 pre-mount). Idle face mounts only when fully Idle.
  return (
    <div ref={stageRef} className={`stage notch-shell ${shellMode}`}>
      {voiceToast ? <div className="voice-toast">{voiceToast}</div> : null}
      {showIdleFace ? (
        <div className="notch-idle" aria-hidden={open || collapsing || expanding}>
          {idleChin}
        </div>
      ) : null}
      <div
        className={`panel${showSettings ? " panel--settings" : ""}`}
        onPointerEnter={cancelAutoCollapse}
        onPointerLeave={onPanelLeave}
        aria-hidden={!open || collapsing || (expanding && !open)}
      >
        <div className="panel__body">
        {showSettings ? (
          <Settings
            appearance={appearance}
            setAppearance={setAppearance}
            showStatusInNotch={showStatusInNotch}
            setShowStatusInNotch={setShowStatusInNotch}
            hasKey={!!status?.has_key}
            keyRejected={!!status?.key_rejected}
            stateCount={state.commitments.length + state.open_loops.length}
            onDone={() => {
              closeSettings();
              refreshLlm();
            }}
            onCleared={refreshState}
          />
        ) : meetingLive?.state === "recording" && open ? (
          <MeetingNote />
        ) : voiceLive && open ? (
          <VoicePanel
            view={voiceLive}
            onDismiss={() => void invoke("voice_dismiss").catch(() => undefined)}
          />
        ) : (
          <>
            <header className="head" onMouseDown={beginDrag}>
              <div className="head__left">
                {/* The live source sits top-left, in the same spot the collapsed pill occupies, so
                    opening the panel doesn't make the indicator jump to the bottom. App NAME only —
                    never window titles or paths (no usernames leak into the UI). */}
                {noticeLine ? (
                  <span className={`srcchip inline--${noticeLine.tone}`}>
                    <span className={`inline__dot inline__dot--${noticeLine.tone}`} />
                    {noticeLine.text}
                    {noticeLine.fix ? (
                      // The reason and the way out in the same chip: a warning that makes the user
                      // hunt for the setting is only marginally better than silence.
                      <button
                        className="chip chip--inline"
                        type="button"
                        onClick={() => runFix(noticeLine.fix)}
                      >
                        {t.healthFix}
                      </button>
                    ) : null}
                  </span>
                ) : live ? (
                  <span className="srcchip" title={`${t.reading} ${live}`}>
                    <span className="live__dot" />
                    {t.reading} <b>{live}</b>
                  </span>
                ) : null}
                {/* Raw counts ("4000 due") read as alarm-noise once real data lands, so the
                    tracked-items list opens from a quiet icon instead of a number badge. */}
                {totalState > 0 ? (
                  <button
                    className={`icon${showState ? " icon--on" : ""}`}
                    type="button"
                    title={t.stateList}
                    aria-label={t.stateList}
                    aria-pressed={showState}
                    onClick={() => setShowState((v) => !v)}
                  >
                    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M4 5.5l1.5 1.5L8 4.5" />
                      <path d="M11 6h9" />
                      <path d="M4 12l1.5 1.5L8 11" />
                      <path d="M11 12.5h9" />
                      <path d="M4 18.5l1.5 1.5L8 17.5" />
                      <path d="M11 19h9" />
                    </svg>
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
                  {pinned ? <IconPin /> : <IconPinOff />}
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
                    <IconHistory />
                  </button>
                ) : null}
                {/* Memory search (B-6). Also on `/` — the button is the discoverable face of the
                    shortcut. */}
                <button
                  className="icon"
                  type="button"
                  title={t.searchOpen}
                  aria-label={t.searchOpen}
                  aria-pressed={showSearch}
                  onClick={() => setShowSearch((v) => !v)}
                >
                  ⌕
                </button>
                {/* The brief, health, memory and run log open HERE, as the in-panel hub — the
                    notch is where things finish; a separate window would defeat that. Only
                    meetings and Visual Recall keep their own surfaces. */}
                <button
                  className={`icon${showHub ? " icon--on" : ""}`}
                  type="button"
                  title={t.overview}
                  aria-label={t.overview}
                  aria-pressed={showHub}
                  onClick={toggleHub}
                >
                  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                       stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="4" y="4" width="7" height="7" rx="1.5" />
                    <rect x="13" y="4" width="7" height="7" rx="1.5" />
                    <rect x="4" y="13" width="7" height="7" rx="1.5" />
                    <rect x="13" y="13" width="7" height="7" rx="1.5" />
                  </svg>
                </button>
                <button className="icon" type="button" title={t.settings} aria-label={t.settings} onClick={openSettings}>
                  <IconSettings />
                  {/* B-1: pending L3 sends. The count is the only thing this touches — the queue
                      itself is the ApprovalsSection's business (Settings). */}
                  {approvalsCount > 0 ? (
                    <span className="icon__badge" title={t.approvalsBadge(approvalsCount)} aria-label={t.approvalsBadge(approvalsCount)}>
                      {approvalsCount}
                    </span>
                  ) : null}
                </button>
                <span className="head__divider" aria-hidden="true" />
                <button className="icon" type="button" title={t.minimize} aria-label={t.minimize} onClick={collapse}>
                  <IconMinimize />
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
                  <IconClose />
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

            {showSearch ? <SearchBox onClose={() => setShowSearch(false)} /> : null}

            {summaryOpen ? (
              // The delivered summary owns the panel until dismissed (issue #10). Opening it IS
              // the read receipt (the card marks itself seen on mount); Done returns to chat.
              <SummaryCard
                which={summaryOpen.which}
                date={summaryOpen.date}
                onClose={() => setSummaryOpen(null)}
              />
            ) : showHub ? (
              <Hub />
            ) : (
              <>
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
              {/* The answer as it is written. The dots only stand in for the wait BEFORE the
                  first token — once text is arriving, the text itself is the progress indicator,
                  and leaving the dots under it would be two things saying the same thing. */}
              {/* aria-atomic=false so a screen reader announces each new piece rather than
                  re-reading the whole answer from the top on every repaint — which, at one
                  repaint per frame, would make the panel unusable with VoiceOver. */}
              {streaming ? (
                <div className="msg msg--shogun msg--live" aria-live="polite" aria-atomic="false">
                  {streaming}
                  <span className="msg__caret" aria-hidden="true" />
                </div>
              ) : thinking ? (
                <div className="msg msg--shogun msg--think" aria-live="polite" aria-label={t.thinkingAria}>
                  <span className="think__dot" />
                  <span className="think__dot" />
                  <span className="think__dot" />
                </div>
              ) : null}
            </div>

            <ActionsRow
              onQueued={() => setApprovalsCount((n) => n + 1)}
              claimSlo={claimActionsSlo}
            />

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
                    {/* Send becomes Stop for the duration of a turn. A disabled send button
                        during the one moment you might want to take it back is a dead control
                        exactly when the user needs a live one — and stopping here really does
                        abandon the request upstream, not just hide the answer. */}
                    {thinking ? (
                      <button
                        className="composer__send composer__send--stop"
                        type="button"
                        title={t.stop}
                        aria-label={t.stop}
                        onClick={stopStreaming}
                      >
                        ■
                      </button>
                    ) : (
                      <button
                        className="composer__send"
                        type="button"
                        title={t.send}
                        aria-label={t.send}
                        onClick={send}
                        disabled={!input.trim()}
                      >
                        ↑
                      </button>
                    )}
                  </div>
                </div>
              </div>
            </div>
              </>
            )}
          </>
        )}
        </div>
        <ResizeGrip
          current={() => (showSettings ? setSize : showHub ? hubSize : chatSize)}
          onResize={onResizeLive}
          onCommit={onResizeCommit}
        />
      </div>
    </div>
  );
}

/** Context-action buttons above the composer (Plan B-1 / E-10). Fetched once per expand — the
 *  cache is pre-assembled Rust-side (never collect-on-press), so this is a read, not a build.
 *  Dispositions: L1 ran (`executed`) → one-line note; L2 (`confirm:<id>`) → inline Confirm/Cancel
 *  chip → `confirm_notch_action`; L3 send (`queued:<id>` / `drafting`) → queued-for-approval note + badge bump
 *  via `onQueued`. Buttons are ordinary tab stops; the row never traps focus. */
function ActionsRow({
  onQueued,
  claimSlo,
}: {
  onQueued: () => void;
  /// Returns true when this mount owns the expand's SLO-02 sample. False on a remount caused by
  /// something other than an expand (a Hub toggle), which must not land in the percentiles.
  claimSlo: () => boolean;
}): JSX.Element | null {
  const [actions, setActions] = useState<ActionView[]>(IN_TAURI ? [] : MOCK_ACTIONS);
  const [busy, setBusy] = useState<number | null>(null);
  /// A pending L2 one-tap confirm: which button asked, and the engine's action id.
  const [confirm, setConfirm] = useState<{ idx: number; id: number } | null>(null);
  const [note, setNote] = useState<{ text: string; tone: "ok" | "warn" } | null>(null);
  const noteTimer = useRef<number | null>(null);

  useEffect(() => {
    if (!IN_TAURI) return;
    const t0 = performance.now();
    let stale = false;
    const measures = claimSlo();
    void invoke<ActionView[]>("notch_actions")
      .then((a) => {
        if (stale) return;
        setActions(a.slice(0, 4));
        if (!measures) return;
        // SLO-02: expand → buttons painted. Double-rAF so the row is actually on screen (same
        // convention as the `painted` command); the sample lands via `record_ui_slo`.
        requestAnimationFrame(() =>
          requestAnimationFrame(() =>
            void invoke("record_ui_slo", { name: "actions_present", ms: performance.now() - t0 }).catch(
              () => undefined,
            ),
          ),
        );
      })
      .catch(() => setActions([]));
    return () => {
      stale = true;
    };
  }, [claimSlo]);
  useEffect(
    () => () => {
      if (noteTimer.current != null) window.clearTimeout(noteTimer.current);
    },
    [],
  );

  const flash = (text: string, tone: "ok" | "warn"): void => {
    setNote({ text, tone });
    if (noteTimer.current != null) window.clearTimeout(noteTimer.current);
    noteTimer.current = window.setTimeout(() => {
      noteTimer.current = null;
      setNote(null);
    }, 2600);
  };

  const run = (idx: number): void => {
    if (busy != null) return;
    const a = actions[idx];
    if (!a) return;
    if (!IN_TAURI) {
      flash(`${t.actionDone} — ${a.label}`, "ok");
      return;
    }
    setBusy(idx);
    setConfirm(null);
    void invoke<string>("run_notch_action", { index: idx })
      .then((r) => {
        if (r === "executed") flash(`${t.actionDone} — ${a.label}`, "ok");
        else if (r.startsWith("confirm:")) setConfirm({ idx, id: Number(r.slice("confirm:".length)) });
        else if (r.startsWith("queued:") || r === "drafting") {
          flash(t.actionQueued, "ok");
          onQueued();
        } else if (r === "rejected") flash(t.actionRejected, "warn");
        else if (r === "no-action") flash(t.actionGone, "warn");
        else flash(t.actionFailed, "warn");
      })
      .catch(() => flash(t.actionFailed, "warn"))
      .finally(() => setBusy(null));
  };

  const doConfirm = (): void => {
    const c = confirm;
    if (!c) return;
    setConfirm(null);
    void invoke<string>("confirm_notch_action", { id: c.id })
      .then((r) => {
        if (r === "executed") flash(`${t.actionDone} — ${actions[c.idx]?.label ?? ""}`.trim(), "ok");
        else if (r === "expired") flash(t.actionExpired, "warn");
        else flash(t.actionFailed, "warn");
      })
      .catch(() => flash(t.actionFailed, "warn"));
  };

  // Fusion's never-empty guarantee means this shouldn't happen — but when it does, no strip is
  // better than an empty one.
  if (actions.length === 0 && !note && !confirm) return null;

  return (
    <div className="acts" role="toolbar" aria-label={t.actionsAria}>
      {note ? (
        <div className={`acts__note acts__note--${note.tone}`} role="status">
          {note.text}
        </div>
      ) : null}
      {confirm ? (
        <div className="acts__confirm" role="group" aria-label={t.actionConfirmQ}>
          <span className="acts__confirmq">
            {t.actionConfirmQ} <b>{actions[confirm.idx]?.label}</b>
          </span>
          <button type="button" className="acts__go" onClick={doConfirm}>
            {t.actionConfirm}
          </button>
          {/* Cancel is local: the engine's pending confirm simply expires server-side (8s), so
              dismissing the chip is enough — nothing runs without the explicit tap. */}
          <button type="button" className="acts__cancel" onClick={() => setConfirm(null)}>
            {t.actionCancel}
          </button>
        </div>
      ) : (
        actions.map((a, i) => (
          <button
            key={i}
            type="button"
            className="acts__btn"
            disabled={busy != null}
            title={a.rationale}
            onClick={() => run(i)}
          >
            <span className={`acts__lvl acts__lvl--${a.level.toLowerCase()}`}>{a.level}</span>
            <span className="acts__label">{busy === i ? "…" : a.label}</span>
          </button>
        ))
      )}
    </div>
  );
}

/** In-panel memory search (Plan B-6 / E-10b). Debounced hybrid search over the event log via
 *  `search_memory`; Enter copies the top match, Esc clears then closes. Records SLO-04 per query
 *  (committed → results drawn) through the same `record_ui_slo` path as the actions row. */
export function SearchBox({ onClose }: { onClose: () => void }): JSX.Element {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHitView[]>([]);
  /// A store failure, not an empty result (issue #121). Held apart from `hits` because the two
  /// mean opposite things: "nothing matched" is an answer, "memory is unavailable" is not.
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState<number | null>(null);
  const inputEl = useRef<HTMLInputElement>(null);
  /// Monotonic query token: a slow early response must never overwrite a newer query's rows.
  const seq = useRef(0);

  useEffect(() => {
    inputEl.current?.focus();
  }, []);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      return;
    }
    if (!IN_TAURI) {
      setHits(MOCK_HITS);
      return;
    }
    const id = window.setTimeout(() => {
      const mine = ++seq.current;
      const t0 = performance.now();
      void invoke<SearchHitView[]>("search_memory", { query: q, limit: 8 })
        .then((r) => {
          if (mine !== seq.current) return;
          setFailed(false);
          setHits(r);
          // SLO-04: query committed → results drawn (double-rAF, as everywhere).
          requestAnimationFrame(() =>
            requestAnimationFrame(() =>
              void invoke("record_ui_slo", { name: "local_search", ms: performance.now() - t0 }).catch(
                () => undefined,
              ),
            ),
          );
        })
        .catch(() => {
          if (mine !== seq.current) return;
          // Rust rejected the read: say the memory is unavailable rather than drawing the same
          // "no matches" the user gets when their memory genuinely holds nothing.
          setHits([]);
          setFailed(true);
        });
    }, 150);
    return () => window.clearTimeout(id);
  }, [query]);

  /// Picking a row copies its excerpt — the panel's cheapest useful act on a memory (the jump to
  /// the evidence row lives in the Full UI).
  const pick = (h: SearchHitView): void => {
    void navigator.clipboard?.writeText(h.excerpt).catch(() => undefined);
    setCopied(h.event_id);
    window.setTimeout(() => setCopied(null), 1600);
  };

  return (
    <div className="search">
      <input
        ref={inputEl}
        className="search__input"
        aria-label={t.searchAria}
        placeholder={t.searchPlaceholder}
        value={query}
        onFocus={() => {
          // Same as the composer: a nonactivating NSPanel won't take keystrokes until made key.
          if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
        }}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && hits.length > 0) {
            e.preventDefault();
            pick(hits[0]);
          } else if (e.key === "Escape") {
            // First Esc clears, second closes — and never reaches the global handler that hides
            // the whole panel.
            e.preventDefault();
            e.stopPropagation();
            if (query) setQuery("");
            else onClose();
          }
        }}
      />
      {query.trim() ? (
        failed ? (
          <div className="search__empty is-warn">{t.searchUnavailable}</div>
        ) : hits.length === 0 ? (
          <div className="search__empty">{t.searchEmpty}</div>
        ) : (
          <div className="search__results">
            {hits.map((h) => (
              <button key={h.event_id} type="button" className="search__row" onClick={() => pick(h)}>
                <span className="search__excerpt">{emphasize(h.excerpt, query)}</span>
                <span className="search__meta">
                  {copied === h.event_id ? t.searchCopied : `${appName(h.app || h.source)} · ${relTime(h.ts)}`}
                </span>
              </button>
            ))}
          </div>
        )
      ) : (
        <div className="search__hintline">{t.searchHint}</div>
      )}
    </div>
  );
}

/** In-panel hub: the overview panes (Today / Health / Sources / Memory / Activity / Trace) drawn
 *  inside the notch panel. Same Rust-assembled `full_ui_view` snapshot, same presentation-only
 *  rule (CLAUDE.md invariant 1) — only the chrome differs: a tab strip instead of a sidebar, at
 *  overlay density. Everything routine finishes in the notch; meetings and Visual Recall are the
 *  deliberate exceptions with their own surfaces. */
/// Hub tabs run in attention order; Context Health and Traceability are audit surfaces you visit
/// rarely, so they fold into one "System" tab at the far right instead of two up front
/// (2026-08-09). The Full UI window keeps them as separate panes.
type HubPane = "today" | "sources" | "memory" | "activity" | "system";
const HUB_TABS: { id: HubPane; label: string }[] = [
  { id: "today", label: tf.navToday },
  { id: "sources", label: tf.navSources },
  { id: "memory", label: tf.navMemory },
  { id: "activity", label: tf.navActivity },
  { id: "system", label: tf.navSystem },
];

/** Context Health + Traceability, folded into one compact pane: the big-window card grid becomes
 *  dense label/value rows with the fix inline, the SLO table an inline strip, and the trace table
 *  sits directly below — presentation-only over the same Rust-assembled view. */
function HubSystem({ v, onNav }: { v: FullUiView; onNav: (p: HubPane) => void }): JSX.Element {
  const h = v.health;
  return (
    <>
      <div className="fcard">
        <div className="fcard__label">{tf.navHealth}</div>
        <div className="sysgrid">
          {h.cards.map((c) => (
            <div className="sysrow" key={c.key}>
              <span className="sysrow__k">{c.label}</span>
              <span className="sysrow__v">
                {c.value}
                {c.detail ? <span className="sysrow__d"> · {c.detail}</span> : null}
              </span>
              {c.fix ? (
                c.fix.target === "sources" ? (
                  <button type="button" className="sysrow__fix" onClick={() => onNav("sources")}>
                    {c.fix.label} →
                  </button>
                ) : (
                  // Settings lives behind ⚙︎ and the trace table is right below — a pointer, not
                  // a button that would go nowhere.
                  <span className="sysrow__fix sysrow__fix--quiet">{c.fix.label}</span>
                )
              ) : null}
            </div>
          ))}
          {h.mix ? (
            <div className="sysrow">
              <span className="sysrow__k">{tf.confidenceMix}</span>
              <span className="sysrow__v sysrow__d">
                {tf.high} {h.mix.high_pct}% · {tf.medium} {h.mix.medium_pct}% · {tf.low} {h.mix.low_pct}%
              </span>
            </div>
          ) : null}
        </div>
        {h.slo.length > 0 ? (
          <div className="sysslo">
            {h.slo.map((s) => (
              <span className="sysslo__item" key={s.name}>
                {s.name}{" "}
                {s.p50 == null ? (
                  <span className="sysslo__t">—</span>
                ) : (
                  <b className={s.within_target ? "" : "is-warn"}>{s.p50}</b>
                )}
                <span className="sysslo__t">/{s.target}</span>
              </span>
            ))}
          </div>
        ) : null}
      </div>
      <HubTrace v={v.trace} />
    </>
  );
}

function Hub(): JSX.Element {
  const [view, setView] = useState<FullUiView | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [pane, setPane] = useState<HubPane>("today");

  // A one-shot snapshot per open, like the old window took per launch — the hub unmounts when
  // you leave it, so reopening refetches.
  useEffect(() => {
    if (!IN_TAURI) {
      setView(SAMPLE_VIEW);
      return;
    }
    invoke<FullUiView>("full_ui_view")
      .then(setView)
      .catch((e) => setFailed(String(e)));
  }, []);

  return (
    <div className="hub">
      <div className="hub__tabs" role="tablist">
        {HUB_TABS.map((n) => (
          <button
            key={n.id}
            type="button"
            role="tab"
            className={`hub__tab${pane === n.id ? " is-on" : ""}`}
            aria-selected={pane === n.id}
            onClick={() => setPane(n.id)}
          >
            {n.label}
          </button>
        ))}
      </div>
      <div className="hub__body">
        {/* Say what went wrong rather than falling back to fixture data — a pane quietly showing
            invented numbers is the one failure mode this surface must not have. */}
        {failed ? (
          <div className="fempty">
            {t.hubFailed} — {failed}
          </div>
        ) : !view ? null : (
          <>
            {pane === "today" && <HubToday v={view.today} />}
            {pane === "sources" && <HubSources v={view.sources} />}
            {pane === "memory" && <HubMemory v={view.memory} />}
            {pane === "activity" && <HubActivity v={view.activity} />}
            {pane === "system" && <HubSystem v={view} onNav={setPane} />}
          </>
        )}
      </div>
    </div>
  );
}

/** Collapsed notch pill while hold-to-talk is active (#44). */
export function VoicePill(): JSX.Element {
  const [scales, setScales] = useState<number[]>([0.35, 0.35, 0.35, 0.35]);

  useEffect(() => {
    const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ?? false;
    if (reduceMotion) return;
    const timer = window.setInterval(() => {
      setScales(Array.from({ length: 4 }, () => 0.35 + Math.random() * 0.65));
    }, 300);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <div className="vpill" role="status" aria-label={t.voiceListening}>
      <span className="vpill__visualizer" aria-hidden>
        {scales.map((scale, index) => (
          <span
            className="vpill__bar"
            key={index}
            style={{ transform: `scaleY(${scale})` }}
          />
        ))}
      </span>
    </div>
  );
}

/** Same compact notch slot, visibly processing rather than recording. */
export function VoiceProcessingPill(): JSX.Element {
  return (
    <div className="vpill" role="status" aria-label={t.voiceProcessing}>
      <span className="vpill__loader" aria-hidden />
    </div>
  );
}

/** Expanded notch surface for voice dialogue (#44). */
export function VoicePanel({
  view,
  onDismiss,
}: {
  view: VoiceView;
  onDismiss: () => void;
}): JSX.Element {
  const copyResponse = (): void => {
    if (!view.response) return;
    void navigator.clipboard.writeText(view.response).catch(() => undefined);
  };

  return (
    <div className="voice-panel">
      {view.phase === "recording" ? (
        <>
          <div className="voice-panel__kicker">{t.voiceListening}</div>
          <div className="voice-panel__meter" aria-hidden>
            <div className="voice-panel__meter-fill" style={{ width: `${Math.round(view.level * 100)}%` }} />
          </div>
          <div className="voice-panel__hint">{t.voiceHoldHint}</div>
        </>
      ) : null}

      {view.phase === "processing" ? (
        <>
          <div className="voice-panel__kicker">{t.voiceProcessing}</div>
          {view.transcript ? <div className="voice-panel__transcript">"{view.transcript}"</div> : null}
        </>
      ) : null}

      {view.phase === "response" ? (
        <>
          <div className="voice-panel__kicker">{t.voiceAnswer}</div>
          {view.transcript ? (
            <div className="voice-panel__transcript voice-panel__transcript--sub">"{view.transcript}"</div>
          ) : null}
          <div className="voice-panel__response">{view.response}</div>
          <div className="voice-panel__acts">
            <button type="button" className="voice-panel__btn" onClick={copyResponse}>
              {t.voiceCopy}
            </button>
            <button type="button" className="voice-panel__btn voice-panel__btn--primary" onClick={onDismiss}>
              {t.voiceClose}
            </button>
          </div>
        </>
      ) : null}

      {view.phase === "error" ? (
        <>
          <div className="voice-panel__kicker">{t.voiceError}</div>
          <div className="voice-panel__response">{view.error || t.voiceError}</div>
          <div className="voice-panel__acts">
            <button type="button" className="voice-panel__btn voice-panel__btn--primary" onClick={onDismiss}>
              {t.voiceClose}
            </button>
          </div>
        </>
      ) : null}
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
        onPointerDown={(e) => {
          // Primary button only — pointerdown fires for right/middle too, and ending a meeting
          // on a stray right-click is not recoverable.
          if (e.button !== 0) return;
          e.stopPropagation();
          e.preventDefault();
          void invoke("meeting_stop").catch(() => undefined);
        }}
        onClick={(e) => {
          e.stopPropagation();
          e.preventDefault();
        }}
      >
        {t.meetingStop}
      </button>
    </div>
  );
}

// Corner grip that lets the user stretch the open panel. The native panel is a borderless NSPanel
// (no OS resize edges), so we drive `set_panel_size` from a pointer drag. Horizontal centre stays
// under the notch: drag Δ on the right edge → width += 2Δ so left mirrors. screenX/Y (not client*)
// — client coords shift when the panel re-centres mid-drag and would double-count.
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
    start.current = { x: e.screenX, y: e.screenY, w: s.w, h: s.h };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onMove = (e: React.PointerEvent): void => {
    const s = start.current;
    if (!s) return;
    const dx = e.screenX - s.x;
    const dy = e.screenY - s.y;
    // Width: ±dx/2 each side (centre fixed) ⇒ total Δw = 2·dx so right edge tracks the cursor.
    // Height: top stays docked to the notch ⇒ Δh = dy only (grows/shrinks downward).
    onResize(s.w + 2 * dx, s.h + dy);
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

// First-layer connections (§6.9): the row UI lives in src/connections.tsx, shared with the
// onboarding flow so the two surfaces can never drift (invariant 1 — presentation only here).

// ── Plan & billing (issue #8) ───────────────────────────────────────────────────────────────
// Display only. The plan gate itself is decided in the Rust core (CLAUDE.md: プラン判定はRust
// コア側で行う) — `entitlement_status` is the resolved answer, `billing_status` is the licence
// behind it. This panel never decides anything; it shows what the core decided and offers the
// two buttons that lead to Stripe's own hosted pages.

interface EntitlementView {
  status: "trial" | "trial_expired" | "standard" | "pro";
  agent_execution: boolean;
  memory_api: boolean;
  composio_send_unlock: boolean;
  first_layer_reads: boolean;
  trial_started_at: number | null;
}

interface BillingView {
  activated: boolean;
  plan: string | null;
  status: string | null;
  valid: boolean;
  offline_grace: boolean;
  days_offline: number;
  amber: boolean;
  current_period_end: number | null;
  verified_at: number | null;
  cancel_at_period_end: boolean;
  error: string | null;
}

/** Unix seconds → a date the reader recognises. */
function billingDate(secs: number | null): string {
  if (!secs) return "";
  const d = new Date(secs * 1000);
  return Number.isNaN(d.getTime()) ? "" : d.toISOString().slice(0, 10);
}

/** The four purchasable combinations. Prices are display copy; the price itself lives server-side. */
const PLAN_CHOICES: { plan: "standard" | "pro"; interval: "annual" | "monthly"; label: string }[] = [
  { plan: "standard", interval: "annual", label: t.planBuyStandardYear },
  { plan: "standard", interval: "monthly", label: t.planBuyStandardMonth },
  { plan: "pro", interval: "annual", label: t.planBuyProYear },
  { plan: "pro", interval: "monthly", label: t.planBuyProMonth },
];

function PlanBillingSection(): JSX.Element {
  const [ent, setEnt] = useState<EntitlementView | null>(null);
  const [billing, setBilling] = useState<BillingView | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const [choosing, setChoosing] = useState(false);

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<EntitlementView>("entitlement_status").then(setEnt).catch(() => undefined);
    void invoke<BillingView>("billing_status").then(setBilling).catch(() => undefined);
  }, []);
  useEffect(refresh, [refresh]);

  const activate = (): void => {
    if (!IN_TAURI || !keyInput.trim()) return;
    setBusy(true);
    setMsg("");
    void invoke<BillingView>("billing_activate", { licenseKey: keyInput.trim() })
      .then((v) => {
        setBilling(v);
        setMsg(v.error ?? "");
        if (!v.error) setKeyInput("");
        // The plan the core enforces may have just changed — re-read it rather than inferring.
        void invoke<EntitlementView>("entitlement_status").then(setEnt).catch(() => undefined);
      })
      .catch((e) => setMsg(String(e)))
      .finally(() => setBusy(false));
  };

  const call = (cmd: string, args?: Record<string, unknown>): void => {
    if (!IN_TAURI) return;
    setBusy(true);
    setMsg("");
    void invoke(cmd, args)
      .then(() => refresh())
      .catch((e) => setMsg(String(e)))
      .finally(() => setBusy(false));
  };

  // The headline is the plan in force — from the core's resolution, not from the licence file.
  const planLabel = ((): string => {
    switch (ent?.status) {
      case "pro":
        return t.planPro;
      case "standard":
        return t.planStandard;
      case "trial_expired":
        return t.planTrialExpired;
      default:
        return t.planTrial;
    }
  })();
  const expired = ent?.status === "trial_expired";
  const paid = ent?.status === "pro" || ent?.status === "standard";

  return (
    <section className="set">
      <div className="set__label">{t.planTitle}</div>
      <div className="conn">
        <div className="conn__meta">
          <span className={`conn__state${expired ? " is-warn" : paid ? " is-ok" : ""}`}>
            {planLabel}
          </span>
          {billing?.status ? (
            <span className="conn__state">
              {t.planStatusLabel}: {billing.status}
            </span>
          ) : null}
          {billing?.current_period_end ? (
            <span className="conn__state">
              {billing.cancel_at_period_end ? t.planEndsOn : t.planNextBilling}:{" "}
              {billingDate(billing.current_period_end)}
            </span>
          ) : null}
        </div>
        <button className="keyrow__btn" type="button" disabled={busy} onClick={() => call("billing_refresh")}>
          {t.planRefresh}
        </button>
      </div>
      <div className="set__hint">{t.planHint}</div>
      {expired ? <div className="set__hint is-warn">{t.planExpiredHint}</div> : null}
      {billing?.cancel_at_period_end ? (
        <div className="set__hint is-warn">{t.planCancelsAtPeriodEnd}</div>
      ) : null}
      {/* Offline grace (FR-BIL-09): amber from day 7, and it says how many days are left rather
          than just "offline", because only the remaining days are actionable. */}
      {billing?.offline_grace ? (
        <div className={`set__hint${billing.amber ? " is-warn" : ""}`}>
          {t.planOffline.replace("{n}", String(billing.days_offline)).replace("{total}", "14")}
        </div>
      ) : null}
      {billing?.verified_at ? (
        <div className="set__hint">
          {t.planLastChecked}: {billingDate(billing.verified_at)}
        </div>
      ) : null}

      {/* Upgrade / manage. Both open Stripe-hosted pages in the browser — no card UI here. */}
      <div className="keyrow">
        {!paid ? (
          <button className="keyrow__btn" type="button" disabled={busy} onClick={() => setChoosing((v) => !v)}>
            {t.planUpgrade}
          </button>
        ) : null}
        {billing?.activated ? (
          <button className="keyrow__btn" type="button" disabled={busy} onClick={() => call("billing_open_portal")}>
            {t.planManage}
          </button>
        ) : null}
      </div>
      {choosing ? (
        <div className="keyrow keyrow--wrap">
          {PLAN_CHOICES.map((c) => (
            <button
              key={`${c.plan}-${c.interval}`}
              className="keyrow__btn"
              type="button"
              disabled={busy}
              onClick={() => call("billing_open_checkout", { plan: c.plan, interval: c.interval })}
            >
              {c.label}
            </button>
          ))}
        </div>
      ) : null}

      {/* Activation. The key comes from the purchase confirmation; it goes straight to the
          Keychain and is never rendered back. */}
      {billing?.activated ? (
        <>
          <div className="set__hint is-ok">{t.planActivated}</div>
          <div className="keyrow">
            <button
              className="keyrow__btn"
              type="button"
              disabled={busy}
              onClick={() => call("billing_deactivate")}
            >
              {t.planDeactivate}
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="set__hint">{t.planActivateTitle}</div>
          <div className="set__hint">{t.planActivateHint}</div>
          <div className="keyrow">
            <input
              className="keyrow__input"
              placeholder={t.planActivatePlaceholder}
              value={keyInput}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setKeyInput(e.target.value)}
              onFocus={() => {
                if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") activate();
              }}
            />
            <button
              className="keyrow__btn"
              type="button"
              onClick={activate}
              disabled={busy || !keyInput.trim()}
            >
              {busy ? t.planActivating : t.planActivate}
            </button>
          </div>
        </>
      )}
      {msg ? <div className="set__hint is-err">{msg}</div> : null}
    </section>
  );
}

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

/** Hold-to-talk dictation (#44). Beta, off by default; transcript goes to focused field or clipboard. */
export function VoiceSection(): JSX.Element {
  const [on, setOn] = useState(false);
  const [busy, setBusy] = useState(false);
  const [edit, setEdit] = useState({ model: "openai/gpt-oss-120b", has_key: false });
  const [editKeyInput, setEditKeyInput] = useState("");
  const [editBusy, setEditBusy] = useState(false);
  const [editMsg, setEditMsg] = useState("");

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<{ enabled: boolean }>("get_voice_settings")
      .then((s) => setOn(s.enabled))
      .catch(() => undefined);
    void invoke<{ model: string; has_key: boolean }>("get_voice_edit_settings")
      .then(setEdit)
      .catch(() => undefined);
  }, []);

  const toggle = (next: boolean): void => {
    if (!IN_TAURI) {
      setOn(next);
      return;
    }
    setBusy(true);
    setOn(next);
    void invoke("set_voice_enabled", { enabled: next })
      .catch(() => setOn(!next))
      .finally(() => setBusy(false));
  };

  const saveEditKey = (): void => {
    const key = editKeyInput.trim();
    if (!key || editBusy) return;
    setEditBusy(true);
    void invoke("set_voice_edit_key", { key })
      .then(() => {
        setEdit((current) => ({ ...current, has_key: true }));
        setEditKeyInput("");
      })
      .catch((err: unknown) => setEditMsg(String(err)))
      .finally(() => setEditBusy(false));
  };

  const clearEditKey = (): void => {
    if (editBusy) return;
    setEditBusy(true);
    void invoke("clear_voice_edit_key")
      .then(() => setEdit((current) => ({ ...current, has_key: false })))
      .catch((err: unknown) => setEditMsg(String(err)))
      .finally(() => setEditBusy(false));
  };

  return (
    <section className="set">
      <div className="set__label">{t.voiceSection}</div>
      <div className="set__hint">{t.voiceHint}</div>
      <div className="seg" role="radiogroup" aria-label={t.voiceSection}>
        <button
          type="button"
          role="radio"
          aria-checked={!on}
          className={`seg__opt${!on ? " is-on" : ""}`}
          disabled={busy}
          onClick={() => toggle(false)}
        >
          {t.voiceOff}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={on}
          className={`seg__opt${on ? " is-on" : ""}`}
          disabled={busy}
          onClick={() => toggle(true)}
        >
          {t.voiceOn}
        </button>
      </div>
      <div className="set__stack set__stack--key">
        <div className="set__label">{t.voiceEditModel}</div>
        <div className="set__hint set__hint--quiet">{edit.model}</div>
        <div className={`set__status${edit.has_key ? " is-ok" : ""}`}>
          {edit.has_key ? t.voiceEditKeyPresent : t.voiceEditKeyAbsent}
        </div>
        <div className="set__sublabel">{t.voiceEditKey}</div>
        <p className="set__hint set__hint--quiet">{t.voiceEditKeyHint}</p>
        {editMsg ? <p className="set__hint is-err">{editMsg}</p> : null}
        <div className="keyrow">
          <input
            className="keyrow__input"
            type="password"
            placeholder={t.voiceEditKeyPlaceholder}
            value={editKeyInput}
            autoComplete="off"
            onChange={(e) => {
              setEditKeyInput(e.target.value);
              setEditMsg("");
            }}
            onFocus={() => {
              if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
            }}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              e.preventDefault();
              saveEditKey();
            }}
          />
          <button
            className="keyrow__btn keyrow__btn--go"
            type="button"
            disabled={!editKeyInput.trim() || editBusy}
            onClick={saveEditKey}
          >
            {t.keySave}
          </button>
          {edit.has_key ? (
            <button
              className="keyrow__btn keyrow__btn--quiet"
              type="button"
              disabled={editBusy}
              onClick={clearEditKey}
            >
              {t.keyRemove}
            </button>
          ) : null}
        </div>
        <p className="set__hint set__hint--quiet">{t.voiceEditModelHint}</p>
      </div>
    </section>
  );
}

/** Sound cues (issue #49). Rust owns the policy — this only reads and writes the three settings
 *  it is allowed to change, and states the one rule it cannot: nothing plays while a microphone
 *  is live, because that chime would land in the user's transcript and in everyone else's call. */
type SoundPref = "off" | "essential" | "full";
interface SoundSettings {
  pref: SoundPref;
  startup_sound: boolean;
  quiet_hours: { enabled: boolean; start_min: number; end_min: number };
}

const SOUND_DEFAULTS: SoundSettings = {
  pref: "essential",
  startup_sound: false,
  quiet_hours: { enabled: true, start_min: 22 * 60, end_min: 8 * 60 },
};

const minToTime = (m: number): string =>
  `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
const timeToMin = (v: string): number => {
  const [h, m] = v.split(":").map((n) => Number.parseInt(n, 10));
  if (!Number.isFinite(h) || !Number.isFinite(m)) return 0;
  return Math.min(23 * 60 + 59, Math.max(0, h * 60 + m));
};

/** Daily summaries (issue #10): morning/evening switches + the evening threshold. Writes return
 *  what Rust actually stored (same contract as the sound settings), so a rejected value never
 *  leaves the UI and the daemon quietly disagreeing. */
function DailySummariesSection(): JSX.Element {
  const [s, setS] = useState<DailySettings>({
    morning_enabled: true,
    evening_enabled: true,
    evening_hour: 17,
    evening_minute: 30,
  });
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<DailySettings>("get_daily_summary_settings")
      .then(setS)
      .catch(() => undefined);
  }, []);

  const write = (next: DailySettings): void => {
    setS(next);
    if (!IN_TAURI) return;
    setBusy(true);
    void invoke<DailySettings>("set_daily_summary_settings", { settings: next })
      .then(setS)
      .catch(() => undefined)
      .finally(() => setBusy(false));
  };

  const pad = (n: number): string => String(n).padStart(2, "0");

  return (
    <section className="set">
      <div className="set__label">{t.dsSection}</div>
      <div className="set__hint">{t.dsHint}</div>
      <label className="set__row">
        <input
          type="checkbox"
          checked={s.morning_enabled}
          disabled={busy}
          onChange={(e) => write({ ...s, morning_enabled: e.target.checked })}
        />
        <span>{t.dsMorning}</span>
      </label>
      <label className="set__row">
        <input
          type="checkbox"
          checked={s.evening_enabled}
          disabled={busy}
          onChange={(e) => write({ ...s, evening_enabled: e.target.checked })}
        />
        <span>{t.dsEvening}</span>
      </label>
      {s.evening_enabled ? (
        <div className="set__row">
          <label>
            {t.dsEveningFrom}{" "}
            <input
              type="time"
              value={`${pad(s.evening_hour)}:${pad(s.evening_minute)}`}
              disabled={busy}
              onChange={(e) => {
                const m = /^(\d{1,2}):(\d{2})$/.exec(e.target.value);
                if (!m) return;
                write({ ...s, evening_hour: Number(m[1]), evening_minute: Number(m[2]) });
              }}
            />
          </label>
        </div>
      ) : null}
    </section>
  );
}

function SoundSection(): JSX.Element {
  const [s, setS] = useState<SoundSettings>(SOUND_DEFAULTS);
  const [busy, setBusy] = useState(false);
  const [muted, setMuted] = useState(false);

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<SoundSettings>("get_sound_settings")
      .then((next) => setS(next))
      .catch(() => undefined);
  }, []);

  /** Every write returns the settings Rust actually stored, so a clamped value shows up here
   *  instead of the UI and the daemon quietly disagreeing. */
  const write = (cmd: string, args: Record<string, unknown>): void => {
    if (!IN_TAURI) return;
    setBusy(true);
    void invoke<SoundSettings>(cmd, args)
      .then((next) => setS(next))
      .catch(() => undefined)
      .finally(() => setBusy(false));
  };

  const preview = (): void => {
    if (!IN_TAURI) return;
    void invoke<boolean>("preview_sound_cue", { asset: "ask" })
      .then((played) => setMuted(!played))
      .catch(() => undefined);
  };

  const hint =
    s.pref === "off" ? t.soundOffHint : s.pref === "full" ? t.soundFullHint : t.soundEssentialHint;

  return (
    <section className="set">
      <div className="set__label">{t.soundSection}</div>
      <div className="set__hint">{t.soundHint}</div>
      <div className="seg" role="radiogroup" aria-label={t.soundSection}>
        {(["off", "essential", "full"] as SoundPref[]).map((p) => (
          <button
            key={p}
            type="button"
            role="radio"
            aria-checked={s.pref === p}
            className={`seg__opt${s.pref === p ? " is-on" : ""}`}
            disabled={busy}
            onClick={() => write("set_sound_pref", { pref: p })}
          >
            {p === "off" ? t.soundOff : p === "essential" ? t.soundEssential : t.soundFull}
          </button>
        ))}
      </div>
      <div className="set__hint">{hint}</div>
      {s.pref !== "off" ? (
        <div className="set">
          <label className="set__row">
            <input
              type="checkbox"
              checked={s.startup_sound}
              disabled={busy}
              onChange={(e) => write("set_sound_startup", { enabled: e.target.checked })}
            />
            <span>{t.soundStartup}</span>
          </label>
          <div className="set__hint">{t.soundStartupHint}</div>

          <label className="set__row">
            <input
              type="checkbox"
              checked={s.quiet_hours.enabled}
              disabled={busy}
              onChange={(e) =>
                write("set_sound_quiet_hours", {
                  enabled: e.target.checked,
                  startMin: s.quiet_hours.start_min,
                  endMin: s.quiet_hours.end_min,
                })
              }
            />
            <span>{t.soundQuietHours}</span>
          </label>
          {s.quiet_hours.enabled ? (
            <div className="set__row">
              <label>
                {t.soundQuietFrom}{" "}
                <input
                  type="time"
                  value={minToTime(s.quiet_hours.start_min)}
                  disabled={busy}
                  onChange={(e) =>
                    write("set_sound_quiet_hours", {
                      enabled: true,
                      startMin: timeToMin(e.target.value),
                      endMin: s.quiet_hours.end_min,
                    })
                  }
                />
              </label>
              <label>
                {t.soundQuietTo}{" "}
                <input
                  type="time"
                  value={minToTime(s.quiet_hours.end_min)}
                  disabled={busy}
                  onChange={(e) =>
                    write("set_sound_quiet_hours", {
                      enabled: true,
                      startMin: s.quiet_hours.start_min,
                      endMin: timeToMin(e.target.value),
                    })
                  }
                />
              </label>
            </div>
          ) : null}

          <div className="set__row">
            <button className="chip" type="button" onClick={preview}>
              {t.soundPreview}
            </button>
          </div>
          {muted ? <div className="set__hint">{t.soundPreviewMuted}</div> : null}
        </div>
      ) : null}
      {/* Not a footnote: without this line, going quiet mid-call reads as a bug. */}
      <div className="set__hint">{t.soundMicNote}</div>
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
  const [deepgramKey, setDeepgramKey] = useState({ has_key: false, key_last4: "" });
  const [deepgramInput, setDeepgramInput] = useState("");
  const [deepgramErr, setDeepgramErr] = useState("");

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
    void invoke<{ has_key: boolean; key_last4: string }>("get_deepgram_key_status")
      .then((s) => {
        setDeepgramKey(s);
        setDeepgramErr("");
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

  const saveDeepgramKey = (): void => {
    const k = deepgramInput.trim();
    if (!k) return;
    if (!IN_TAURI) {
      setDeepgramKey({ has_key: true, key_last4: k.slice(-4) });
      setDeepgramInput("");
      return;
    }
    void invoke("set_deepgram_key", { key: k })
      .then(() => {
        setDeepgramInput("");
        load();
      })
      .catch((e) => setDeepgramErr(String(e)));
  };

  const removeDeepgramKey = (): void => {
    if (!IN_TAURI) {
      setDeepgramKey({ has_key: false, key_last4: "" });
      return;
    }
    void invoke("clear_deepgram_key")
      .then(load)
      .catch((e) => setDeepgramErr(String(e)));
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
      <p className="set__hint">{t.meetingHint}</p>

      {on ? (
        <div className="set__stack">
          <label className="set__row">
            <input
              type="checkbox"
              checked={micOnly}
              onChange={(e) => toggleMicOnly(e.target.checked)}
            />
            <span className="set__row-label">{t.meetingMicOnly}</span>
          </label>
          <p className="set__hint set__hint--quiet">{t.meetingMicOnlyHint}</p>
          {/* Tier (b), undoable. An exclusion added by an impatient tap during a meeting would
              otherwise become a permanent blind spot with no way back (FR-MT-02b). */}
          <div className="mexcl">
            <div className="set__sublabel">{t.meetingExcluded}</div>
            {excluded.length === 0 ? (
              <p className="set__hint set__hint--quiet">{t.meetingExcludedEmpty}</p>
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
        </div>
      ) : null}

      <div className="set__stack set__stack--key">
        <div className="set__label">{t.deepgramAsrKey}</div>
        <div className={`set__status${deepgramKey.has_key ? " is-ok" : ""}`}>
          {deepgramKey.has_key
            ? `${t.deepgramAsrPresent} ·· ${deepgramKey.key_last4}`
            : t.deepgramAsrAbsent}
        </div>
        <p className="set__hint set__hint--quiet">{t.deepgramAsrHint}</p>
        {deepgramErr ? <p className="set__hint is-err">{deepgramErr}</p> : null}
        <div className="keyrow">
          <input
            className="keyrow__input"
            type="password"
            placeholder={t.deepgramAsrPlaceholder}
            value={deepgramInput}
            autoComplete="off"
            onChange={(e) => setDeepgramInput(e.target.value)}
            onFocus={() => {
              if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") saveDeepgramKey();
            }}
          />
          <button
            className="keyrow__btn keyrow__btn--go"
            type="button"
            onClick={saveDeepgramKey}
            disabled={!deepgramInput.trim()}
          >
            {t.keySave}
          </button>
          {deepgramKey.has_key ? (
            <button
              className="keyrow__btn keyrow__btn--quiet"
              type="button"
              onClick={removeDeepgramKey}
            >
              {t.keyRemove}
            </button>
          ) : null}
        </div>
      </div>

      {/* Kept visible whether the feature is on or off: someone deciding whether to turn it on
          needs this more than someone who already has (FR-MT-03). */}
      <p className="set__disclosure">{t.meetingDisclosure}</p>
    </section>
  );
}

function DockVisibleSection(): JSX.Element {
  const [on, setOn] = useState(true);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<boolean>("get_dock_visible")
      .then(setOn)
      .catch(() => undefined);
  }, []);

  const toggle = (next: boolean): void => {
    if (!IN_TAURI) {
      setOn(next);
      return;
    }
    setBusy(true);
    setOn(next);
    void invoke("set_dock_visible", { visible: next })
      .then(() => invoke<boolean>("get_dock_visible").then(setOn))
      .catch(() => setOn(!next))
      .finally(() => setBusy(false));
  };

  return (
    <section className="set">
      <div className="set__label" id="seg-dock">{t.showInDock}</div>
      <div className="seg" role="radiogroup" aria-labelledby="seg-dock">
        <button
          type="button"
          role="radio"
          aria-checked={on}
          disabled={busy}
          className={`seg__opt${on ? " is-on" : ""}`}
          onClick={() => toggle(true)}
        >
          {t.showInDockOn}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!on}
          disabled={busy}
          className={`seg__opt${!on ? " is-on" : ""}`}
          onClick={() => toggle(false)}
        >
          {t.showInDockOff}
        </button>
      </div>
      <div className="set__hint">{t.showInDockHint}</div>
    </section>
  );
}

function LaunchAtLoginSection(): JSX.Element {
  const [on, setOn] = useState(true);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<{ enabled: boolean }>("get_launch_at_login_settings")
      .then((s) => setOn(s.enabled))
      .catch(() => undefined);
  }, []);

  const toggle = (next: boolean): void => {
    if (!IN_TAURI) {
      setOn(next);
      return;
    }
    setBusy(true);
    setOn(next);
    void invoke("set_launch_at_login_enabled", { enabled: next })
      .then(() =>
        invoke<{ enabled: boolean }>("get_launch_at_login_settings").then((s) => setOn(s.enabled)),
      )
      .catch(() => setOn(!next))
      .finally(() => setBusy(false));
  };

  return (
    <section className="set">
      <div className="set__label" id="seg-launch">{t.launchAtLoginSection}</div>
      <div className="seg" role="radiogroup" aria-labelledby="seg-launch">
        <button
          type="button"
          role="radio"
          aria-checked={on}
          disabled={busy}
          className={`seg__opt${on ? " is-on" : ""}`}
          onClick={() => toggle(true)}
        >
          {t.launchAtLoginOn}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!on}
          disabled={busy}
          className={`seg__opt${!on ? " is-on" : ""}`}
          onClick={() => toggle(false)}
        >
          {t.launchAtLoginOff}
        </button>
      </div>
      <div className="set__hint">{t.launchAtLoginHint}</div>
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
          latest.app ?? t.someApp,
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
        <span className="vr-launch__glyph" aria-hidden="true">
          <IconMaximize2 size={16} />
        </span>
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

interface UserConfigStatus {
  exists: boolean;
  path: string;
  last_updated_ms: number | null;
  ok: boolean;
  errors: { section: string; line: number; message: string }[];
}

function PersonalizationSection(): JSX.Element {
  const [status, setStatus] = useState<UserConfigStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const refresh = (): void => {
    if (!IN_TAURI) return;
    void invoke<UserConfigStatus>("get_user_config_status").then(setStatus).catch(() => undefined);
  };
  useEffect(refresh, []);

  return (
    <section className="set">
      <div className="set__label">{t.personalizationTitle}</div>
      <div className="set__hint">{t.personalizationHint}</div>
      {status ? (
        <>
          <div className={`set__hint${status.ok ? " is-ok" : " is-err"}`}>
            {status.exists
              ? status.ok
                ? t.personalizationOk
                : t.personalizationError(
                    status.errors[0]?.section ?? "",
                    status.errors[0]?.line ?? 0,
                  )
              : t.personalizationMissing}
          </div>
          <div className="set__row">
            <button
              className="keyrow__btn"
              type="button"
              disabled={!status.exists || busy}
              onClick={() => void invoke("open_shougun_md").catch((e) => setErr(String(e)))}
            >
              {t.personalizationOpen}
            </button>
            <button
              className="keyrow__btn"
              type="button"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void invoke("regenerate_shougun_md")
                  .then(refresh)
                  .catch((e) => setErr(String(e)))
                  .finally(() => setBusy(false));
              }}
            >
              {t.personalizationReset}
            </button>
          </div>
          {err ? <div className="set__hint is-err">{err}</div> : null}
        </>
      ) : null}
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
  return (
    <section className="set">
      <div className="set__label">{t.connections}</div>
      <div className="set__hint">{t.connectionsHint}</div>
      <ConnectionsList />
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
  origin: string; // "ui" | "api" | "mcp" — which surface enqueued it (B-3 shared queue)
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
                {/* B-3: which surface enqueued it — a UI, API, or MCP caller share this one
                    queue. An unattributable request reads as UNKNOWN and is marked: the badge is
                    a security disclosure, so the missing case must look stranger, not safer. */}
                <span
                  className={`appr__route${!r.origin || r.origin === "unknown" ? " is-warn" : ""}`}
                >
                  {(r.origin ?? "unknown").toUpperCase()}
                </span>
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

/** A vendor CLI SHOGUN can delegate the Agent lane to, as reported by `subscription_delegates`. */
type DelegateInfo = {
  id: string;
  label: string;
  /** Whose quota a run is billed against, e.g. "Claude Pro / Max". */
  plan: string;
  /** The binary looked up on PATH — shown in the "not installed" instructions. */
  binary: string;
  state: "not_installed" | "installed" | "ready" | "needs_login" | "rate_limited";
  version: string;
};

/** The one line that tells the user where they stand with a delegate, and what to do next. */
function delegateStateLine(state: DelegateInfo["state"]): string {
  switch (state) {
    case "ready":
      return t.subStateReady;
    case "installed":
      return t.subStateInstalled;
    case "needs_login":
      return t.subStateNeedsLogin;
    case "rate_limited":
      return t.subStateRateLimited;
    default:
      return t.subStateNotInstalled;
  }
}
const SHORTCUT_ROWS: Array<{ action: string; label: string }> = [
  { action: "draft", label: t.draftShortcut },
  { action: "recall", label: t.recallShortcut },
  { action: "summon", label: t.summonShortcut },
  { action: "voice", label: t.voiceShortcut },
  { action: "quit", label: t.quitShortcut },
];
/** Actions whose trigger may be a bare-modifier gesture (a solo tap or a left+right pair) rather
 *  than a key chord. Matches the Rust side's special-combo handling. */
const GESTURE_ACTIONS = new Set(["draft", "recall"]);


/** Privacy & Security (issue #28). One place for the LLM key, the data-use policy, and data
 *  deletion. The BYOK key entry lived inline in `Settings`; it moves here so the key, its
 *  "encrypted in the Keychain" promise, and the deletion controls all read as one privacy story.
 *  The key is never shown back in plaintext — settled state is a set/not-set indicator only. */
function PrivacySecuritySection(props: {
  hasKey: boolean;
  /// The provider refused this key — surfaced here, since this is where the fix is.
  keyRejected: boolean;
  /// Called after a "delete everything & account" wipe so the parent re-reads key status —
  /// that command removes every provider's Keychain key, so `hasKey` would otherwise go stale.
  onDeleted: () => void;
  /// Agent-lane provider + model, owned by `Settings`. NOT re-fetched here: a second copy of
  /// this state meant picking a subscription delegate in the parent card left `provider` stale
  /// down here, and `saveKey` passes it explicitly — so the key landed in the WRONG provider's
  /// Keychain account. One owner, one value.
  provider: string;
  /// Apply a provider change through the parent (blank model = that provider's default).
  /// Rejects when the backend refuses, so the key card can show the reason where the user is
  /// looking.
  onApplyLlm: (provider: string, model: string) => Promise<void>;
}): JSX.Element {
  const { hasKey, keyRejected, onDeleted, provider, onApplyLlm } = props;
  // BYOK key entry: the key goes straight to the macOS Keychain via Rust (never a file/DB/log).
  // Only the last 4 chars are ever read back (invariant 7 / NFR-SEC-02) — the full key stays in
  // Rust — so the read-back echoes "Connected ··1234", mirroring the Composio card in this screen.
  const [keyInput, setKeyInput] = useState("");
  const [keyState, setKeyState] = useState<boolean>(hasKey);
  const [keyLast4, setKeyLast4] = useState("");
  const [keyMsg, setKeyMsg] = useState("");
  useEffect(() => setKeyState(hasKey), [hasKey]);
  // Fetch the active provider's last-4 for the read-back echo. Re-runs whenever key presence
  // flips (save/remove/wipe all funnel through `keyState`) so the suffix stays accurate.
  const refreshLast4 = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<string | null>("byok_key_last4")
      .then((l4) => setKeyLast4(l4 ?? ""))
      .catch(() => setKeyLast4(""));
  }, []);
  useEffect(() => {
    if (keyState) refreshLast4();
    else setKeyLast4("");
  }, [keyState, refreshLast4]);
  const applyLlm = (p: string, m: string): void => {
    setKeyMsg("");
    void onApplyLlm(p, m).catch((e: unknown) => setKeyMsg(String(e)));
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

  // Anonymous usage — the SAME opt-out state (analytics.json) the onboarding toggle and the
  // PostHog worker read (opt-out model, default ON; CLAUDE.md 2026-08-08 統合決定). `analytics`
  // here is "enabled" = !opt_out. Writing rolls the local state back on error so the toggle
  // never claims a state the backend rejected.
  // Data deletion (A3). 1h/24h are single-tap-then-confirm; "all" wipes everything and the keys,
  // so it mirrors the deliberate typed-confirmation used for clearing extracted state.
  const [confirming, setConfirming] = useState<null | "1h" | "24h" | "all">(null);
  const [deleteMsg, setDeleteMsg] = useState("");
  const [deleteText, setDeleteText] = useState("");
  // In-flight guard: the confirm buttons stay disabled until the delete command settles, so a
  // second click can't fire delete_data_since / delete_all_and_account again mid-flight.
  const [deleting, setDeleting] = useState(false);
  const canDeleteAll = deleteText.trim().toUpperCase() === "DELETE";
  const runDelete = (which: "1h" | "24h" | "all"): void => {
    if (deleting) return;
    setDeleteMsg("");
    setDeleting(true);
    if (!IN_TAURI) {
      setDeleteMsg(t.deleteDone);
      setConfirming(null);
      setDeleteText("");
      // A full wipe removes the key too — reflect that in the mock, matching the real command.
      if (which === "all") setKeyState(false);
      setDeleting(false);
      return;
    }
    const call =
      which === "all"
        ? invoke("delete_all_and_account")
        : invoke("delete_data_since", { range: which });
    void call
      .then(() => {
        setDeleteMsg(t.deleteDone);
        setConfirming(null);
        setDeleteText("");
        if (which === "all") {
          setKeyState(false);
          // The wipe cleared every provider's Keychain key — have the parent re-read status so
          // its `hasKey` (and anything gated on it) doesn't stay stale.
          onDeleted();
        }
      })
      .catch((e) => setDeleteMsg(String(e)))
      .finally(() => setDeleting(false));
  };

  return (
    <section className="set">
      <div className="set__label">{t.privacyTitle}</div>

      {/* LLM API Key card. Provider picker + hidden key entry + set/not-set indicator. */}
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
              // Model ids are provider-specific — blank = the provider's default.
              if (p.id !== provider) applyLlm(p.id, "");
            }}
          >
            {p.label}
          </button>
        ))}
      </div>
      <div className="set__hint">
        {t.modelFor} {defaultModelFor(provider)}
      </div>
      <div className="set__hint">{t.modelHint}</div>
      <div className={`set__hint${keyRejected ? " is-err" : keyState ? " is-ok" : ""}`}>
        {keyRejected
          ? t.keyRejected
          : keyState
            ? keyLast4
              ? `${t.keyPresent} ·· ${keyLast4}`
              : t.keyPresent
            : t.keyAbsent}
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
      <div className="set__hint">{t.keyEncryptedNote}</div>

      {/* Data-use policy card. */}
      <div className="badges">
        <span className="badge">{t.policyNotTrained}</span>
        <span className="badge">{t.policyLocalFirst}</span>
        <span className="badge">{t.policyEncrypted}</span>
      </div>
      <div className="set__hint">
        <a
          href="https://shogunai.app/privacy"
          target="_blank"
          rel="noopener noreferrer"
        >
          {t.policyLink}
        </a>
      </div>

      {/* Data deletion card (A3). Local and immediate. */}
      <div className="set__label">{t.deleteTitle}</div>
      <div className="set__hint">{t.deleteHint}</div>
      {confirming === null ? (
        <div className="keyrow">
          <button className="keyrow__btn" type="button" onClick={() => setConfirming("1h")}>
            {t.deleteLast1h}
          </button>
          <button className="keyrow__btn" type="button" onClick={() => setConfirming("24h")}>
            {t.deleteLast24h}
          </button>
          <button
            className="keyrow__btn keyrow__btn--danger"
            type="button"
            onClick={() => setConfirming("all")}
          >
            {t.deleteAll}
          </button>
        </div>
      ) : confirming === "all" ? (
        // "All" wipes everything and the keys — deliberate typed confirmation.
        <div className="confirm">
          <div className="set__hint is-err">{t.deleteAllConfirm}</div>
          <div className="keyrow">
            <input
              className="keyrow__input"
              placeholder={t.deleteAllConfirmPlaceholder}
              value={deleteText}
              autoFocus
              autoComplete="off"
              onFocus={() => {
                if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
              }}
              onChange={(e) => setDeleteText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && canDeleteAll && !deleting) runDelete("all");
                if (e.key === "Escape") {
                  setConfirming(null);
                  setDeleteText("");
                }
              }}
            />
            <button
              className="keyrow__btn"
              type="button"
              onClick={() => {
                setConfirming(null);
                setDeleteText("");
              }}
            >
              {t.cancel}
            </button>
            <button
              className="keyrow__btn keyrow__btn--danger"
              type="button"
              onClick={() => runDelete("all")}
              disabled={!canDeleteAll || deleting}
            >
              {t.deleteConfirmBtn}
            </button>
          </div>
        </div>
      ) : (
        // 1h / 24h — single confirm step (destructive but bounded). Name the window being deleted.
        <div className="confirm">
          <div className="set__hint is-err">
            {t.deleteConfirmRange.replace(
              "{range}",
              confirming === "1h" ? t.deleteLast1h : t.deleteLast24h,
            )}
          </div>
          <div className="keyrow">
            <button
              className="keyrow__btn"
              type="button"
              onClick={() => setConfirming(null)}
            >
              {t.cancel}
            </button>
            <button
              className="keyrow__btn keyrow__btn--danger"
              type="button"
              onClick={() => runDelete(confirming)}
              disabled={deleting}
            >
              {t.deleteConfirmBtn}
            </button>
          </div>
        </div>
      )}
      {deleteMsg ? <div className="set__hint is-ok">{deleteMsg}</div> : null}

      {/* Anonymous usage. ONE control, the shared AnalyticsToggle — three widgets over the same
          analytics.json opt_out drifted out of sync within a single scroll of Settings. */}
      <div className="set__label">{t.analyticsTitle}</div>
      <AnalyticsToggle />
    </section>
  );
}

export function NotchStatusSection({
  visible,
  onVisibleChange,
}: {
  visible: boolean;
  onVisibleChange: (visible: boolean) => void;
}): JSX.Element {
  const [busy, setBusy] = useState(false);

  const setNotchStatus = (next: boolean): void => {
    const previous = visible;
    onVisibleChange(next);
    if (!IN_TAURI) return;
    setBusy(true);
    void invoke("set_notch_status_visible", { visible: next })
      .catch(() => onVisibleChange(previous))
      .finally(() => setBusy(false));
  };

  return (
    <section className="set">
      <div className="set__label" id="seg-notch-status">{t.notchStatus}</div>
      <div className="seg" role="radiogroup" aria-labelledby="seg-notch-status">
        <button
          type="button"
          role="radio"
          aria-checked={visible}
          disabled={busy}
          className={`seg__opt${visible ? " is-on" : ""}`}
          onClick={() => setNotchStatus(true)}
        >
          {t.notchStatusShow}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!visible}
          disabled={busy}
          className={`seg__opt${!visible ? " is-on" : ""}`}
          onClick={() => setNotchStatus(false)}
        >
          {t.notchStatusHide}
        </button>
      </div>
      <p className="set__hint">{t.notchStatusHint}</p>
    </section>
  );
}

function Settings(props: {
  appearance: Appearance;
  setAppearance: (a: Appearance) => void;
  showStatusInNotch: boolean;
  setShowStatusInNotch: (v: boolean) => void;
  hasKey: boolean;
  /// The provider refused this key — shown in the key section, since that is where the fix is.
  keyRejected: boolean;
  stateCount: number;
  onDone: () => void;
  onCleared: () => void;
}): JSX.Element {
  const {
    appearance,
    setAppearance,
    showStatusInNotch,
    setShowStatusInNotch,
    hasKey,
    keyRejected,
    stateCount,
    onDone,
    onCleared,
  } = props;
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
  // Errors from delegate selection/verification surface inside the subscription card. BYOK key
  // entry itself lives in PrivacySecuritySection — the one key home (2026-08-08 統合).
  const [subMsg, setSubMsg] = useState("");
  // Agent-lane provider + model (non-secret; the key is per-provider in the Keychain).
  const [provider, setProvider] = useState("anthropic");
  const [model, setModel] = useState("");
  // Subscription delegation (Issue #110). `consent` is the user's acceptance of the disclosure;
  // the backend refuses to delegate without it, so this is not a cosmetic checkbox.
  const [delegates, setDelegates] = useState<DelegateInfo[]>([]);
  const [consent, setConsent] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  const loadDelegates = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<DelegateInfo[]>("subscription_delegates")
      .then(setDelegates)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<{ provider: string; model: string; subscription_consent: boolean }>("get_llm_settings")
      .then((s) => {
        setProvider(s.provider);
        setModel(s.model);
        setConsent(s.subscription_consent);
      })
      .catch(() => undefined);
    loadDelegates();
  }, [loadDelegates]);
  /// The ONE writer for the agent-lane provider/model (and the delegation consent). Returns a
  /// promise so a caller that owns its own error surface — the key card in
  /// `PrivacySecuritySection` — can show the failure where the user is looking, instead of in a
  /// card they may have scrolled past.
  const applyLlm = (p: string, m: string, c?: boolean): Promise<void> => {
    const prev = { provider, model, consent };
    setProvider(p);
    setModel(m);
    if (c !== undefined) setConsent(c);
    setSubMsg("");
    if (!IN_TAURI) return Promise.resolve();
    return invoke("set_llm_settings", { provider: p, model: m, subscriptionConsent: c }).then(
      () => undefined,
      (e: unknown) => {
        // Roll the UI back — an optimistic provider the backend never accepted would send the
        // next key save to the wrong Keychain account.
        setProvider(prev.provider);
        setModel(prev.model);
        setConsent(prev.consent);
        setSubMsg(String(e));
        throw e;
      },
    );
  };
  /** Run one real completion through a delegate to find out whether it is signed in. */
  const testDelegate = (id: string): void => {
    if (!IN_TAURI) return;
    setTesting(id);
    void invoke<DelegateInfo>("verify_subscription_delegate", { id })
      .then((d) => setDelegates((prev) => prev.map((p) => (p.id === d.id ? d : p))))
      .catch((e) => setSubMsg(String(e)))
      .finally(() => setTesting(null));
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
  //
  // Draft and Visual recall additionally accept bare-modifier gestures: a solo modifier tap
  // (press and release one modifier with nothing else) records "Tap+X", and pressing the left
  // and right side of the same modifier together records "Dual+X" — matching how they trigger.
  useEffect(() => {
    if (!recording) return;
    const action = recording;
    const gestures = GESTURE_ACTIONS.has(action);
    /// Physical modifier codes currently held (e.g. "MetaLeft"). `solo` is the tap candidate: the
    /// one modifier that went down alone; any other input clears it. `dirty` marks that a
    /// non-modifier key joined this hold, killing both gestures until everything is released.
    const down = new Set<string>();
    let solo: string | null = null;
    let dirty = false;
    const modOf = (code: string): string | null =>
      code.startsWith("Control") ? "Control"
      : code.startsWith("Alt") ? "Alt"
      : code.startsWith("Shift") ? "Shift"
      : code.startsWith("Meta") ? "Super"
      : null;
    const commit = (combo: string): void => {
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
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(null);
        setKeyErr("");
        return;
      }
      const mod = modOf(e.code);
      if (["Control", "Alt", "Shift", "Meta"].includes(e.key) || mod) {
        if (!mod) return;
        down.add(e.code);
        if (gestures) {
          const base = e.code.replace(/(Left|Right)$/, "");
          if (down.has(base + "Left") && down.has(base + "Right")) {
            commit("Dual+" + mod);
            return;
          }
          solo = down.size === 1 && !dirty ? e.code : null;
        }
        return; // modifier alone: keep waiting for the rest of the chord (or a tap release)
      }
      dirty = true;
      solo = null;
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
      commit([...mods, e.code].join("+"));
    };
    const onUp = (e: KeyboardEvent): void => {
      const mod = modOf(e.code);
      if (!mod) return;
      if (gestures && solo === e.code && !dirty && down.size === 1) {
        commit("Tap+" + mod);
        return;
      }
      down.delete(e.code);
      if (down.size === 0) {
        solo = null;
        dirty = false;
      }
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onUp, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onUp, true);
    };
  }, [recording, refresh]);

  return (
    <div className="settings">
      <header className="settings__head">
        <span className="settings__title">{t.settings}</span>
        <button className="settings__done" type="button" onClick={onDone}>
          {t.done}
        </button>
      </header>
      <div className="settings__body">
        <ApprovalsSection />
        {/* Plan state first: when a trial has ended, everything below it is locked, and the
            reason has to be the first thing on screen rather than a discovery. */}
        <PlanBillingSection />
        {/* Near the top on purpose. Meeting notes ships off and only ever turns on because
            someone found this switch — burying an opt-in below six connectors is how a feature
            stays permanently off (FR-MT-01). */}
        <MeetingSection />
        <LaunchAtLoginSection />
        <VisualRecallSection />
        <VoiceSection />
        <SoundSection />
        <DailySummariesSection />
        <ConnectionsSection />
        <ComposioSection />
        <AiSessionsSection />
        <DreamSection />
        <PersonalizationSection />
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
        <DockVisibleSection />
        <CastlePositionSection />
        <NotchStatusSection visible={showStatusInNotch} onVisibleChange={setShowStatusInNotch} />
        <section className="set">
          <div className="set__label">{t.shortcuts}</div>
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
                  {/* comboChips, not a local formatter: it is the only thing that knows the
                      gesture grammar (`Tap+Alt` → "⌥ tap", `Dual+Super` → "⌘ ⌘"), and a second
                      one here rendered those two as the literal words "Tap ⌥" / "Dual ⌘" — a
                      different shortcut from the one onboarding teaches. */}
                  <span className="keys__combo">{comboChips(binds[action] ?? "").join(" ")}</span>
                </button>
              )}
            </div>
          ))}
          {keyErr ? <div className="set__hint is-err">{keyErr}</div> : null}
          <p className="set__hint set__hint--quiet">{t.shortcutHint}</p>
        </section>
        {/* Subscription first, API key second. The order is the point: most people arriving here
            already pay for an assistant, and asking them for a metered key before offering the
            plan they hold is what makes them close the window. */}
        <section className="set">
          <div className="set__label">{t.subTitle}</div>
          <div className="set__hint">{t.subHint}</div>
          {delegates.filter((d) => d.state !== "not_installed").length === 0 ? (
            <div className="set__hint">{t.subNone}</div>
          ) : (
            delegates
              .filter((d) => d.state !== "not_installed")
              .map((d) => {
                const active = provider === d.id;
                return (
                  <div className="sub" key={d.id}>
                    <div className="sub__head">
                      <span className="sub__name">{d.label}</span>
                      <span className="sub__plan">
                        {t.subRunsOn} {d.plan}
                      </span>
                    </div>
                    <div
                      className={`set__hint${d.state === "ready" ? " is-ok" : d.state === "needs_login" ? " is-err" : ""}`}
                    >
                      {delegateStateLine(d.state)}
                    </div>
                    <div className="keyrow">
                      <button
                        className="keyrow__btn"
                        type="button"
                        disabled={active}
                        // Selecting carries the consent already on file. If it was never given the
                        // backend refuses to delegate, and the note below says so — the alternative
                        // (auto-granting on click) would make the disclosure meaningless.
                        onClick={() => applyLlm(d.id, "")}
                      >
                        {active ? t.subInUse : t.subUse}
                      </button>
                      <button
                        className="keyrow__btn"
                        type="button"
                        disabled={testing === d.id}
                        onClick={() => testDelegate(d.id)}
                      >
                        {testing === d.id ? t.subTesting : t.subTest}
                      </button>
                    </div>
                  </div>
                );
              })
          )}
          <div className="keyrow">
            <button className="keyrow__btn" type="button" onClick={loadDelegates}>
              {t.subRefresh}
            </button>
          </div>
          <div className="set__label">{t.subConsentTitle}</div>
          <ul className="set__list">
            <li>{t.subConsentItem1}</li>
            <li>{t.subConsentItem2}</li>
            <li>{t.subConsentItem3}</li>
          </ul>
          <div className="keyrow">
            <button
              className="keyrow__btn"
              type="button"
              onClick={() => applyLlm(provider, model, !consent)}
            >
              {consent ? t.subConsentRevoke : t.subConsentAccept}
            </button>
          </div>
          {/* `delegates` lists every known delegate, installed or not, so this is a reliable
              "is a subscription selected" test without duplicating the Rust-side list here. */}
          {delegates.some((d) => d.id === provider) && !consent ? (
            <div className="set__hint is-err">{t.subConsentNeeded}</div>
          ) : null}
          {subMsg ? <div className="set__hint is-err">{subMsg}</div> : null}
        </section>
        <PrivacySecuritySection
          hasKey={hasKey}
          keyRejected={keyRejected}
          onDeleted={onCleared}
          provider={provider}
          onApplyLlm={(p, m) => applyLlm(p, m)}
        />
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
