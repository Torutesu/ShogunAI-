// Browser preview: a mock of the Tauri IPC bridge.
//
// The panel's design work happens in a browser, not on device — a Mac with the entitlements,
// a Keychain key and live connectors is a slow loop for judging spacing and colour. This module
// installs a fake `window.__TAURI_INTERNALS__` so `apps/desktop/src/App.tsx` — the REAL component,
// unmodified — believes it is running inside Tauri and renders every state the design needs to be
// judged in: connected services, a pending approval, a nightly run that failed, a rejected key.
//
// IMPORTANT: nothing here ships in the app bundle. It is reachable only through `preview.html`
// (the Tauri window loads `index.html`), and it is the ONLY place in the preview that fakes data —
// invariant 1 (the data layer lives in Rust) is not weakened, because the preview has no data
// layer at all, only a scripted stand-in for one.
//
// The bridge is installed as a module side effect on purpose: `IN_TAURI` in App.tsx is a
// module-level const, so the internals object must exist before App.tsx is evaluated.
// `main.tsx` therefore imports this module first, and `preview.html` also seeds a placeholder.

export type ConnState = "connected" | "needs_reauth" | "disconnected" | "coming_soon";

export interface ServiceStatus {
  source: string;
  state: ConnState;
  last_sync_ms: number | null;
  has_endpoint: boolean;
}
export interface ApprovalView {
  id: number;
  op_type: string;
  destination: string;
  full_body: string;
  route: string;
}
export interface StateItem {
  id: number;
  text: string;
  meta: string;
}
export interface DreamStatus {
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

/** Panel geometry, driven by the App's own `set_panel_size` calls — the preview never guesses it. */
export interface PanelBox {
  w: number;
  h: number;
  /** Distance from the desk's left edge, in desk points. */
  left: number;
}

export interface OnboardingScenario {
  completed: boolean;
  step: string;
  plan: "standard" | "pro" | null;
  /** What the machine would answer for Accessibility — the branch the flow is judged on. */
  axGranted: boolean;
  draftStop: boolean;
}

export interface Scenario {
  /** Bundle id of the app SHOGUN is "reading" — pushed to the App as a `context` event. */
  foreground: string;
  hasKey: boolean;
  keyRejected: boolean;
  commitments: StateItem[];
  openLoops: StateItem[];
  connections: ServiceStatus[];
  approvals: ApprovalView[];
  dream: DreamStatus;
  aiSessions: boolean;
  provider: string;
  model: string;
  shortcuts: Record<string, string>;
  /** Simulated IPC round-trip, so the UI is judged with the latency it will really have. */
  latencyMs: number;
  panel: PanelBox;
  onboarding: OnboardingScenario;
}

/** 14" MacBook Pro logical points — the panel is laid out at its true size, never a rough scale. */
export const DESK_W = 1512;
export const DESK_H = 944;
export const NOTCH_W = 202;
export const NOTCH_H = 34;

const HOUR = 3600_000;
const NOW = Date.UTC(2026, 6, 25, 9, 41, 0);

const DEFAULT_CONNECTIONS: ServiceStatus[] = [
  { source: "gmail", state: "connected", last_sync_ms: NOW - 6 * 60_000, has_endpoint: true },
  { source: "gcal", state: "connected", last_sync_ms: NOW - 6 * 60_000, has_endpoint: true },
  { source: "gdrive", state: "needs_reauth", last_sync_ms: NOW - 31 * HOUR, has_endpoint: true },
  { source: "slack", state: "disconnected", last_sync_ms: null, has_endpoint: true },
  { source: "notion", state: "coming_soon", last_sync_ms: null, has_endpoint: false },
  { source: "github", state: "coming_soon", last_sync_ms: null, has_endpoint: false },
  { source: "linear", state: "coming_soon", last_sync_ms: null, has_endpoint: false },
];

const DEFAULT_APPROVAL: ApprovalView = {
  id: 1,
  op_type: "Send email",
  destination: "alice@northwind.example",
  route: "composio",
  full_body:
    "Subject: Q3 deck — sending it over\n\nHi Alice,\n\nHere's the Q3 deck we talked about on Tuesday. " +
    "The revenue slide now uses the updated forecast, and I've cut the appendix down to the two " +
    "charts you asked for.\n\nHappy to walk through it Thursday if that's easier.\n\n— Kenji",
};

const DEFAULT_DREAM: DreamStatus = {
  indicator: "normal",
  batch_lane: true,
  last_kind: "full",
  last_cycle_id: "20260725T0312",
  last_succeeded: true,
  last_ended_at: NOW - 6 * HOUR,
  jobs_done: 14,
  jobs_failed: 0,
  duration_ms: 214_000,
  events_processed: 1284,
  state_changes: 9,
  chunks_sent: 42,
  done_tonight: true,
};

export const INITIAL: Scenario = {
  foreground: "com.apple.mail",
  hasKey: true,
  keyRejected: false,
  commitments: [
    { id: 1, text: "Send Alice the Q3 deck", meta: "overdue" },
    { id: 2, text: "Reply to the vendor about pricing", meta: "70% sure" },
    { id: 3, text: "Confirm Thursday's review time", meta: "today" },
  ],
  openLoops: [{ id: 1, text: "Waiting on legal sign-off", meta: "3d waiting" }],
  connections: DEFAULT_CONNECTIONS,
  approvals: [DEFAULT_APPROVAL],
  dream: DEFAULT_DREAM,
  aiSessions: true,
  provider: "anthropic",
  model: "",
  shortcuts: { summon: "Control+Alt+KeyN", quit: "Control+Alt+KeyQ" },
  latencyMs: 60,
  panel: { w: 600, h: 400, left: Math.round((DESK_W - 600) / 2) },
  // Default: already onboarded, so the preview opens on the panel. The rail restarts first run.
  onboarding: { completed: true, step: "welcome", plan: null, axGranted: false, draftStop: true },
};

type Listener = () => void;

class Store {
  private state: Scenario = INITIAL;
  private listeners = new Set<Listener>();

  get = (): Scenario => this.state;

  set = (patch: Partial<Scenario> | ((s: Scenario) => Partial<Scenario>)): void => {
    const next = typeof patch === "function" ? patch(this.state) : patch;
    this.state = { ...this.state, ...next };
    this.listeners.forEach((l) => l());
  };

  subscribe = (l: Listener): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };
}

export const store = new Store();

// ---------------------------------------------------------------------------
// Event plumbing (`@tauri-apps/api/event` → `plugin:event|listen`)
// ---------------------------------------------------------------------------

interface Sub {
  event: string;
  handler: (e: { event: string; id: number; payload: unknown }) => void;
}
const subs = new Map<number, Sub>();
const callbacks = new Map<number, (payload: unknown) => void>();
let nextId = 1;

/** Push a Rust-side event into the webview, exactly as the daemon would. */
export function emit(event: string, payload: unknown): void {
  subs.forEach((s, id) => {
    if (s.event === event) s.handler({ event, id, payload });
  });
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/** Commands whose only job is a side effect on device; in the browser they are a no-op + a log. */
const NOOP = new Set([
  "ui_log",
  "interact",
  "clock_sync_ack",
  "focus_field",
  "start_panel_drag",
  "inline_at_cursor",
  "hide_panel",
  "quit_app",
]);

/** Recent IPC traffic, surfaced in the preview's rail so a silent failure is never invisible. */
export const ipcLog: Array<{ cmd: string; at: number }> = [];

function handle(cmd: string, args: Record<string, unknown>): unknown {
  const s = store.get();
  switch (cmd) {
    case "set_panel_size": {
      const w = Number(args.width);
      const h = Number(args.height);
      const anchor = String(args.anchor ?? "center");
      store.set((cur) => ({
        panel: {
          w,
          h,
          left: anchor === "left" ? cur.panel.left : Math.round((DESK_W - w) / 2),
        },
      }));
      return null;
    }
    case "shogun_status":
      return {
        app: s.foreground,
        commitments: s.commitments.length,
        open_loops: s.openLoops.length,
        has_key: s.hasKey,
        key_rejected: s.keyRejected,
      };
    case "shogun_state":
      return { commitments: s.commitments, open_loops: s.openLoops };
    case "resolve_state_item": {
      const id = Number(args.id);
      if (args.kind === "commitment") {
        store.set({ commitments: s.commitments.filter((c) => c.id !== id) });
      } else {
        store.set({ openLoops: s.openLoops.filter((l) => l.id !== id) });
      }
      return null;
    }
    case "shogun_chat":
      return mockAnswer(String(args.message ?? ""), s);
    case "get_llm_settings":
      return { provider: s.provider, model: s.model };
    case "set_llm_settings":
      store.set({ provider: String(args.provider), model: String(args.model ?? "") });
      return null;
    case "get_shortcuts":
      return s.shortcuts;
    case "set_shortcut":
      store.set({ shortcuts: { ...s.shortcuts, [String(args.action)]: String(args.combo) } });
      return null;
    case "set_byok_key":
      store.set({ hasKey: true, keyRejected: false });
      return null;
    case "clear_byok_key":
      store.set({ hasKey: false, keyRejected: false });
      return null;
    case "clear_memory":
      store.set({ commitments: [], openLoops: [] });
      return null;
    case "connectors_list":
      return s.connections;
    case "connect_service": {
      const src = String(args.service);
      store.set({
        connections: s.connections.map((c) =>
          c.source === src ? { ...c, state: "connected", last_sync_ms: NOW } : c,
        ),
      });
      return null;
    }
    case "disconnect_service": {
      const src = String(args.service);
      store.set({
        connections: s.connections.map((c) =>
          c.source === src ? { ...c, state: "disconnected", last_sync_ms: null } : c,
        ),
      });
      return null;
    }
    case "list_approvals":
      return s.approvals;
    case "confirm_send":
    case "reject_send":
      store.set({ approvals: s.approvals.filter((a) => a.id !== Number(args.id)) });
      return "ok";
    case "dream_status":
      return s.dream;
    case "run_dream_now":
      store.set({
        dream: { ...s.dream, indicator: "normal", last_succeeded: true, last_ended_at: Date.now() },
      });
      return null;
    // ── onboarding ──────────────────────────────────────────────────────
    case "onboarding_state":
      return { completed: s.onboarding.completed, step: s.onboarding.step, plan: s.onboarding.plan };
    case "set_onboarding_state":
      store.set({
        onboarding: {
          ...s.onboarding,
          step: String(args.step),
          plan: (args.plan as "standard" | "pro" | null) ?? null,
          completed: Boolean(args.completed),
        },
      });
      return null;
    case "ax_permission":
      return s.onboarding.axGranted;
    case "request_ax_permission":
      // On device this opens System Settings and the user grants it there. Here it is the wait
      // itself that needs reviewing, so the grant lands a beat later rather than instantly.
      setTimeout(
        () => store.set((cur) => ({ onboarding: { ...cur.onboarding, axGranted: true } })),
        2200,
      );
      return null;
    case "exclusion_categories":
      return [
        { id: "password_managers", count: 6 },
        { id: "auth_dialog", count: 1 },
        { id: "terminals", count: 5 },
        { id: "private_browsing", count: 4 },
        { id: "sensitive_titles", count: 3 },
      ];
    case "get_draft_stop":
      return s.onboarding.draftStop;
    case "set_draft_stop":
      store.set({ onboarding: { ...s.onboarding, draftStop: Boolean(args.enabled) } });
      return null;

    case "get_ai_session_import":
      return s.aiSessions;
    case "set_ai_session_import":
      store.set({ aiSessions: Boolean(args.enabled) });
      return Boolean(args.enabled);
    default:
      if (NOOP.has(cmd)) return null;
      throw new Error(`preview: no mock for command "${cmd}"`);
  }
}

/** A grounded-looking answer, so the message + citation layout is judged with real-length copy. */
function mockAnswer(q: string, s: Scenario): { text: string; citations: unknown[] } {
  const overdue = s.commitments.find((c) => c.meta === "overdue");
  const text = overdue
    ? `Start with ${overdue.text.toLowerCase()} — it's the only thing that's actually late, and ` +
      `Alice asked twice. The deck is already in Drive with Tuesday's numbers; I can draft the ` +
      `covering note where your cursor is and leave it for you to send.`
    : `Nothing is late right now. The vendor thread is the next thing worth closing — you said ` +
      `you'd come back on pricing, and it's been quiet for two days.`;
  return {
    text,
    citations: [
      { event_id: 8821, source: "gmail", title: "Q3 deck — following up" },
      { event_id: 8804, source: "gcal", title: "Review — Thu 14:00" },
      { event_id: 8790, source: "screen", title: q.slice(0, 24) || "Mail" },
    ],
  };
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

interface Internals {
  invoke: (cmd: string, args?: Record<string, unknown>, opts?: unknown) => Promise<unknown>;
  transformCallback: (cb?: (r: unknown) => void, once?: boolean) => number;
  unregisterCallback: (id: number) => void;
  convertFileSrc: (p: string) => string;
}

const internals: Internals = {
  invoke(cmd, args = {}) {
    ipcLog.unshift({ cmd, at: Date.now() });
    ipcLog.length = Math.min(ipcLog.length, 40);
    if (cmd === "plugin:event|listen") {
      const id = nextId++;
      const cb = callbacks.get(Number(args.handler));
      subs.set(id, {
        event: String(args.event),
        handler: (e) => cb?.(e),
      });
      return Promise.resolve(id);
    }
    if (cmd === "plugin:event|unlisten") {
      subs.delete(Number(args.eventId));
      return Promise.resolve(null);
    }
    if (cmd.startsWith("plugin:")) return Promise.resolve(null);
    // Geometry must land in the same frame the App asked for it, or every open/close would
    // visibly lag behind the animation. Everything else pays the simulated round-trip.
    const delay = cmd === "set_panel_size" ? 0 : store.get().latencyMs;
    return new Promise((resolve, reject) => {
      const run = (): void => {
        try {
          resolve(handle(cmd, args));
        } catch (err) {
          reject(err);
        }
      };
      if (delay <= 0) run();
      else setTimeout(run, delay);
    });
  },
  transformCallback(cb) {
    const id = nextId++;
    if (cb) callbacks.set(id, cb as (r: unknown) => void);
    return id;
  },
  unregisterCallback(id) {
    callbacks.delete(id);
  },
  convertFileSrc: (p) => p,
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Partial<Internals>;
  }
}

// Side effect, deliberately at module scope — see the header note about `IN_TAURI`.
window.__TAURI_INTERNALS__ = Object.assign(window.__TAURI_INTERNALS__ ?? {}, internals);
// Unlisten does NOT go through invoke — it calls this object. Without it every unmounted listener
// threw, which is invisible until something in the preview remounts the App.
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: (_event, id) => {
    subs.delete(id);
  },
};

// The App's appearance defaults to "auto", which follows the BROWSER's colour scheme — so a
// reviewer on a light desktop would open the preview and judge the light theme by accident.
// Seed dark for a first visit only; Settings (and the stage control) still own it after that.
try {
  if (!localStorage.getItem("shogun.appearance")) {
    localStorage.setItem("shogun.appearance", JSON.stringify("dark"));
  }
} catch {
  /* private mode: the App falls back to "auto" on its own */
}
