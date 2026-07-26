// Placeholder data for developing the Full UI before the Rust side serves it.
//
// This exists ONLY so the window can be opened and reviewed today. The real view arrives from the
// core over `invoke` (CLAUDE.md invariant 1) — when that lands, delete this file and the branch in
// fullui.tsx that reads it. Nothing outside the dev entry may import it.
//
// The numbers are shaped to exercise the states that are easy to get wrong: a source that needs
// re-auth, a low-confidence row that must stay hedged, an SLO metric that doesn't apply to the
// plan, and a locked action sitting alongside live ones.

import type { FullUiView } from "./types";

export const SAMPLE_VIEW: FullUiView = {
  plan: "pro",

  health: {
    cards: [
      { key: "coverage", label: "Coverage", value: "18h / 24h captured", detail: null,
        fix: { label: "Open capture rules", target: "settings" } },
      { key: "blind", label: "Blind spots", value: "2h focused in an unconnected app",
        detail: "3 events with no source", fix: { label: "Unmute · report as a gap", target: "sources" } },
      { key: "fresh", label: "Freshness", value: "Mail 3m · Calendar 12m · Slack —", detail: null,
        fix: { label: "Re-authenticate Slack", target: "sources" } },
      { key: "yield", label: "Yield", value: "1,204 → 38 → 9 tracked", detail: null,
        fix: { label: "Language settings", target: "settings" } },
      { key: "grounding", label: "Grounding", value: "71% of answers cited a source", detail: null,
        fix: { label: "Widen the search window", target: "settings" } },
      { key: "egress", label: "Egress", value: "12 chunks · 84 KB", detail: "0 third-party",
        fix: { label: "Open Traceability", target: "trace" } },
    ],
    mix: { high_pct: 0, medium_pct: 62, low_pct: 38 },
    slo: [
      { name: "Panel expand", p50: 58, p95: 92, target: "100ms", within_target: true },
      { name: "Local search", p50: 120, p95: 410, target: "500ms", within_target: true },
      { name: "Action buttons", p50: 88, p95: 134, target: "150ms", within_target: true },
      { name: "Cache refresh", p50: 96, p95: 240, target: "300ms", within_target: true },
      { name: "First token", p50: 340, p95: 820, target: "1s", within_target: true },
      { name: "Idle CPU", p50: 1.4, p95: 3.8, target: "5%", within_target: true },
    ],
  },

  today: {
    generated: true,
    sections: [
      { heading: "Today", bullets: [],
        body: "Three meetings, and the vendor renewal needs to close before the launch review on the 14th. Your afternoon is clear after 3 PM — that's the window for the deck." },
      { heading: "Commitments due", body: null,
        bullets: ["Send the revised deck to Alice — due today", "Confirm the launch date with the team — due Friday"] },
      { heading: "Open loops", body: null, bullets: ["Vendor hasn't replied on renewal terms — waiting 3 days"] },
    ],
    actions: [
      { id: "a1", label: "Open the renewal thread", locked: false },
      { id: "a2", label: "Prep for Weekly sync", locked: false },
      { id: "a3", label: "Draft the deck email", locked: true },
    ],
    schedule: [
      { id: "s1", time: "10:00", title: "Weekly sync", detail: "3 people · recurring" },
      { id: "s2", time: "13:30", title: "Vendor call — renewal terms", detail: "Alice Reyes · the open thread" },
    ],
  },

  sources: {
    sources: [
      { id: "mail", name: "Mail", mark: "M", tint: "#EA4335", scope: "read + draft", freshness: "3m ago",
        health: "ok", third_party: false },
      { id: "cal", name: "Calendar", mark: "C", tint: "#1A73E8", scope: "read", freshness: "12m ago",
        health: "ok", third_party: false },
      { id: "slack", name: "Slack", mark: "S", tint: "#611F69", scope: "read", freshness: "token expired",
        health: "warn", third_party: false },
      { id: "send", name: "Mail sending", mark: "G", tint: "#16181D", scope: "send only", freshness: "—",
        health: "ok", third_party: true },
    ],
    exclusions: [
      { id: "e1", title: "Password managers & banking", detail: "Always excluded — can't be turned off.",
        locked: true, enabled: true },
      { id: "e2", title: "Private browsing windows", detail: "Excluded by default.", locked: false, enabled: true },
      { id: "e3", title: "Custom exclusions", detail: "2 apps, 1 window title pattern.", locked: false, enabled: true },
    ],
    ai_sessions_on: true,
  },

  memory: {
    commitments: [
      { id: "c1", text: "You: send the revised deck to Alice", detail: "From Weekly sync · 3 segments · due today",
        confidence: "medium" },
      { id: "c2", text: "Alice: owns the vendor thread", detail: "From Weekly sync · 2 segments", confidence: "medium" },
      { id: "c3", text: "review Q3 numbers before the board call", detail: "From Mail · 1 segment", confidence: "low" },
    ],
    merge_candidates: [
      { id: "m1", names: "Alice Reyes · A. Reyes · alice@vendor.com", detail: "3 records · first seen 12 days ago" },
    ],
  },

  activity: {
    pending: [
      { id: "p1", title: "Send email — Alice Reyes", level: "L3",
        detail: "Drafted 9:12 AM · leaves the device via a third party" },
    ],
    runs: [
      { id: "r1", time: "09:12", action: "Draft reply — vendor renewal", level: "L2", approved_by: "One-tap",
        result: "done", egress: "3 chunks" },
      { id: "r2", time: "08:55", action: "Extract commitments from Weekly sync", level: "L1",
        approved_by: "Automatic", result: "done", egress: null },
      { id: "r3", time: "02:14", action: "Nightly review — reclassify open loops", level: "L1",
        approved_by: "Automatic", result: "done", egress: "9 chunks" },
    ],
    nightly: { finished_at: "2:14 AM", events_read: 1204, updates: 38, chunks_sent: 9, health: "ok" },
  },

  trace: {
    rows: [
      { id: "t1", time: "09:14", route: "third_party", purpose: "Send email", destination: "Composio",
        digest: "sha256:4f9c…a12b", bytes: "1.2 KB" },
      { id: "t2", time: "09:12", route: "direct", purpose: "Draft generation", destination: "Your provider",
        digest: "sha256:9b21…7e04", bytes: "18 KB" },
      { id: "t3", time: "02:14", route: "direct", purpose: "Nightly classification", destination: "Batch indexing",
        digest: "sha256:1ae8…d550", bytes: "41 KB" },
    ],
    third_party_count: 1,
  },
};

/** The same day seen from Standard. Not a second mock so much as the same data with the parts the
 *  plan doesn't include removed by the core: no agent runs (FR-AG-18 is Pro), no third-party
 *  egress, integrations read-only, and the one write action left visible but locked (FR-CF-05). */
export const SAMPLE_VIEW_STANDARD: FullUiView = {
  ...SAMPLE_VIEW,
  plan: "standard",
  today: {
    ...SAMPLE_VIEW.today,
    actions: SAMPLE_VIEW.today.actions.map((a) =>
      a.id === "a3" ? { ...a, locked: true } : { ...a, locked: false },
    ),
  },
  sources: {
    ...SAMPLE_VIEW.sources,
    sources: SAMPLE_VIEW.sources.sources
      .filter((s) => !s.third_party)
      .map((s) => ({ ...s, scope: "read-only" })),
  },
  activity: {
    pending: [],
    runs: [],
    nightly: SAMPLE_VIEW.activity.nightly,
  },
  trace: {
    rows: SAMPLE_VIEW.trace.rows.filter((r) => r.route === "direct"),
    third_party_count: 0,
  },
};
