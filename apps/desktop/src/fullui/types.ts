// Full UI view models.
//
// These are the shapes the Rust core hands to the webview — one per pane, already assembled.
// CLAUDE.md invariant 1: the data layer lives in Rust, so nothing here derives, aggregates, or
// filters. If a number needs computing, it is computed in the core and arrives ready to draw.
//
// Field names are snake_case to match the serde payloads the core emits (same convention as the
// existing ContextPayload / MeetingView in App.tsx).

/** Which plan the account is on. Gating is decided in Rust (FR-BIL-*); the webview only draws
 *  what the core says is available, and never infers entitlement from feature data. */
export type Plan = "trial" | "standard" | "pro";

export type PaneId = "today" | "health" | "sources" | "memory" | "activity" | "trace";

/** Confidence band for an extracted state row (FR-ST-20). `low` rows are shown with the
 *  "possibly:" prefix and must not be turned into suggested actions. */
export type Confidence = "high" | "medium" | "low";

/** Health status for a source or a run. */
export type Health = "ok" | "warn" | "down";

// ——— D2 · Context Health ———

/** One health card. `fix` is the "way to fix it" every card must carry (spec §D2); it is absent
 *  only for SLO, which is read-only. */
export interface HealthCard {
  key: string;
  label: string;
  /** Pre-formatted by the core so the webview never does unit math. */
  value: string;
  detail: string | null;
  fix: { label: string; target: PaneId | "settings" } | null;
}

export interface SloRow {
  name: string;
  /** Unitless — `target` carries the unit, because not every row is milliseconds (idle CPU is a
   *  percentage). Null means the metric doesn't apply on this plan, e.g. first token on Standard
   *  where there is no agent to stream one. */
  p50: number | null;
  p95: number | null;
  target: string;
  within_target: boolean;
}

export interface ConfidenceMix {
  high_pct: number;
  medium_pct: number;
  low_pct: number;
}

export interface HealthView {
  cards: HealthCard[];
  mix: ConfidenceMix;
  slo: SloRow[];
}

// ——— D1 · Today ———

export interface BriefSection {
  heading: string;
  /** Prose paragraph, or bullet lines — the core decides which shape a section is. */
  body: string | null;
  bullets: string[];
}

export interface SuggestedAction {
  id: string;
  label: string;
  /** True when the action needs BYOK and the plan doesn't include it (FR-CF-05). At most one
   *  locked action may be present — the core enforces that cap, not the webview. */
  locked: boolean;
}

export interface ScheduleItem {
  id: string;
  time: string;
  title: string;
  detail: string;
}

export interface TodayView {
  /** False when the nightly review didn't finish: the brief degrades to calendar + overdue only,
   *  with no generated prose (spec §D1). */
  generated: boolean;
  sections: BriefSection[];
  actions: SuggestedAction[];
  schedule: ScheduleItem[];
}

// ——— D4 · Sources ———

export interface SourceRow {
  id: string;
  name: string;
  /** Single letter drawn in the tinted tile — we don't ship approximated trademarks. */
  mark: string;
  tint: string;
  scope: string;
  freshness: string;
  health: Health;
  /** Routed through a third party (Composio) rather than connected directly. */
  third_party: boolean;
}

export interface ExclusionRow {
  id: string;
  title: string;
  detail: string;
  /** Always-on exclusions (password managers, banking) can't be switched off. */
  locked: boolean;
  enabled: boolean;
}

export interface SourcesView {
  sources: SourceRow[];
  exclusions: ExclusionRow[];
  ai_sessions_on: boolean;
}

// ——— D3 · Memory ———

export interface StateRow {
  id: string;
  text: string;
  detail: string;
  confidence: Confidence;
}

export interface MergeCandidate {
  id: string;
  names: string;
  detail: string;
}

export interface MemoryView {
  commitments: StateRow[];
  merge_candidates: MergeCandidate[];
}

// ——— D5 · Activity ———

export type ActionLevel = "L1" | "L2" | "L3";

export interface RunRow {
  id: string;
  time: string;
  action: string;
  level: ActionLevel;
  approved_by: string;
  result: "done" | "failed" | "rejected" | "cancelled";
  /** Pre-formatted egress ("3 chunks") or null when nothing left the device. */
  egress: string | null;
}

export interface PendingApproval {
  id: string;
  title: string;
  detail: string;
  level: ActionLevel;
}

export interface NightlyCycle {
  finished_at: string;
  events_read: number;
  updates: number;
  chunks_sent: number;
  health: Health;
}

export interface ActivityView {
  pending: PendingApproval[];
  /** Empty on Standard — the agent engine isn't part of the plan, so there is no history to show
   *  rather than an empty table (FR-AG-18 is Pro-only). */
  runs: RunRow[];
  nightly: NightlyCycle;
}

// ——— D6 · Traceability ———

export interface EgressRow {
  id: string;
  time: string;
  route: "direct" | "third_party";
  purpose: string;
  destination: string;
  /** Digest only. The body is never logged, so it can't leak from this screen. */
  digest: string;
  bytes: string;
}

export interface TraceView {
  rows: EgressRow[];
  third_party_count: number;
}

// ——— the whole window ———

export interface FullUiView {
  plan: Plan;
  today: TodayView;
  health: HealthView;
  sources: SourcesView;
  memory: MemoryView;
  activity: ActivityView;
  trace: TraceView;
}
