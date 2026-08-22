// Full UI — the separate window (spec §D). Six panes: Today, Context Health, Sources, Memory,
// Activity, Traceability.
//
// The pane components are exported: the notch panel's in-panel hub (App.tsx) draws the same six
// panes inside the panel, so routine work finishes in the notch without this window.
//
// This file is presentation only. Every number it draws arrives pre-computed on a `FullUiView`
// from the Rust core (CLAUDE.md invariant 1) — nothing here aggregates, filters by plan, or
// derives a value. Plan gating is likewise a property of the data: when the core omits agent runs
// because the plan doesn't include the agent engine, the pane renders the "nothing runs here"
// state rather than deciding entitlement for itself.

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatedLogo } from "../Logo";
import { tf } from "../strings";
import type {
  ActivityView,
  Confidence,
  FullUiView,
  HealthView,
  MemoryView,
  PaneId,
  SourcesView,
  TodayView,
  TraceView,
} from "./types";

const NAV: { id: PaneId; label: string; group?: string }[] = [
  { id: "today", label: tf.navToday },
  { id: "health", label: tf.navHealth },
  { id: "sources", label: tf.navSources, group: tf.groupContext },
  { id: "memory", label: tf.navMemory },
  { id: "activity", label: tf.navActivity, group: tf.groupDid },
  { id: "trace", label: tf.navTrace },
];

const PANE_COPY: Record<PaneId, { title: string; sub: string }> = {
  today: { title: tf.navToday, sub: tf.todaySub },
  health: { title: tf.navHealth, sub: tf.healthSub },
  sources: { title: tf.navSources, sub: tf.sourcesSub },
  memory: { title: tf.navMemory, sub: tf.memorySub },
  activity: { title: tf.navActivity, sub: tf.activitySub },
  trace: { title: tf.navTrace, sub: tf.traceSub },
};

export function FullUi({ view }: { view: FullUiView }): JSX.Element {
  // Land on Today — the sidebar's first item. Landing on a different pane than the one the nav
  // highlights first reads as a glitch.
  const [pane, setPane] = useState<PaneId>("today");
  const copy = PANE_COPY[pane];

  return (
    <div className="full">
      <div className="full__body">
        <nav className="side">
          <div className="side__brand">
            <AnimatedLogo size={24} morphTo="heart" hoverWithin=".side__brand" />
            <span className="side__name">ShogunAI</span>
          </div>
          {view.plan !== "pro" && <span className="side__plan">{planLabel(view.plan)}</span>}
          {NAV.map((n) => (
            <div key={n.id}>
              {n.group && <div className="side__group">{n.group}</div>}
              <button
                type="button"
                className={`side__item${pane === n.id ? " is-on" : ""}`}
                aria-current={pane === n.id}
                onClick={() => setPane(n.id)}
              >
                {n.label}
              </button>
            </div>
          ))}
        </nav>

        <section className="pane">
          {/* Keyed so the heading replays its entrance on every pane change. */}
          <div className="pane__head" key={pane}>
            <div className="pane__title">{copy.title}</div>
            <div className="pane__sub">{copy.sub}</div>
          </div>
          <div className="pane__body">
            {pane === "today" && <Today v={view.today} />}
            {pane === "health" && <Health v={view.health} onNav={setPane} />}
            {pane === "sources" && <Sources v={view.sources} />}
            {pane === "memory" && <Memory v={view.memory} />}
            {pane === "activity" && <Activity v={view.activity} />}
            {pane === "trace" && <Trace v={view.trace} />}
          </div>
        </section>
      </div>
    </div>
  );
}

/** Says what would be here and what produces it. A blank card reads as a broken screen. */
function Empty({ children }: { children: string }): JSX.Element {
  return <div className="fempty">{children}</div>;
}

function planLabel(plan: FullUiView["plan"]): string {
  return plan === "trial" ? tf.planTrial : tf.planStandard;
}

// ——— D2 · Context Health ———————————————————————————————————————————————————————————————
// The pane the spec calls the point of the product: every number carries a way to fix it.

export function Health({ v, onNav }: { v: HealthView; onNav: (p: PaneId) => void }): JSX.Element {
  if (v.cards.length === 0 && !v.mix && v.slo.length === 0) {
    return <div className="fcard"><Empty>{tf.emptyHealth}</Empty></div>;
  }
  return (
    <div className="hgrid">
      {v.cards.map((c) => (
        <div className="hcard" key={c.key}>
          <div className="hcard__k">{c.label}</div>
          <div className="hcard__v">
            {c.value}
            {c.detail && <div className="frow__d">{c.detail}</div>}
          </div>
          {c.fix &&
            (c.fix.target === "settings" ? (
              // Capture rules / search window live in the panel's Settings, not in this window —
              // a plain pointer, not a button that would go nowhere.
              <span className="frow__d">{c.fix.label} — SHOGUN panel ⚙︎</span>
            ) : (
              <button type="button" className="hcard__fix" onClick={() => onNav(c.fix!.target as PaneId)}>
                {c.fix.label} →
              </button>
            ))}
        </div>
      ))}

      {/* Absent until the nightly classifier has tallied a night. Saying so beats drawing an
          empty bar that looks like a real 0/0/0 split. */}
      <div className="hcard">
        <div className="hcard__k">{tf.confidenceMix}</div>
        {v.mix ? (
          <div className="hcard__v">
            <div className="mix">
              <span style={{ width: `${v.mix.high_pct}%`, background: "var(--live)" }} />
              <span style={{ width: `${v.mix.medium_pct}%`, background: "var(--accent)" }} />
              <span style={{ width: `${v.mix.low_pct}%`, background: "var(--faint)" }} />
            </div>
            <span className="frow__d">
              {tf.high} {v.mix.high_pct}% · {tf.medium} {v.mix.medium_pct}% · {tf.low} {v.mix.low_pct}%
            </span>
          </div>
        ) : (
          <div className="hcard__v frow__d">{tf.notMeasuredYet}</div>
        )}
      </div>

      <div className="hcard hcard--wide">
        <div className="hcard__k">{tf.slo}</div>
        {v.slo.length === 0 && <div className="hcard__v frow__d">{tf.notMeasuredYet}</div>}
        <div className="hcard__v" style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "8px 26px" }}>
          {v.slo.map((s) => (
            <div key={s.name} style={{ display: "flex", justifyContent: "space-between", fontSize: "var(--fs-md)" }}>
              <span style={{ color: "var(--muted)" }}>{s.name}</span>
              {s.p50 == null ? (
                <span style={{ color: "var(--faint)" }}>{tf.notOnThisPlan}</span>
              ) : (
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  <span style={{ color: s.within_target ? "var(--live)" : "var(--warn)" }}>{s.p50}</span>
                  {" · "}
                  {s.p95} <span style={{ color: "var(--faint)" }}>/ {s.target}</span>
                </span>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ——— D1 · Today ————————————————————————————————————————————————————————————————————————

export function Today({ v }: { v: TodayView }): JSX.Element {
  return (
    <>
      <div className="fcard">
        <div className="fcard__label">{tf.morningBrief}</div>
        {v.never_run ? (
          <div className="fcard__sub">{tf.briefNeverRun}</div>
        ) : (
          !v.generated && <div className="fcard__sub">{tf.briefDegraded}</div>
        )}
        {v.sections.map((s) => (
          <div className="brief__sec" key={s.heading}>
            <div className="brief__h">{s.heading}</div>
            {s.body && <p className="brief__p">{s.body}</p>}
            {s.bullets.map((b) => (
              <div className="brief__b" key={b}>
                <span style={{ color: "var(--faint)" }}>·</span>
                <span>{b}</span>
              </div>
            ))}
          </div>
        ))}
        {v.actions.length === 0 && !v.never_run && (
          <div className="brief__sec">
            <div className="brief__h">{tf.suggested}</div>
            <Empty>{tf.emptyBriefActions}</Empty>
          </div>
        )}
        {v.actions.length > 0 && (
          <div className="brief__sec">
            <div className="brief__h">{tf.suggested}</div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 4 }}>
              {v.actions.map((a) =>
                a.locked ? (
                  // Locked actions stay in place rather than disappearing — the suggestion is
                  // still the right one, it just needs a key to run (FR-CF-05).
                  <span key={a.id} className="fbtn" style={{ color: "var(--faint)", cursor: "default" }}>
                    {a.label}
                  </span>
                ) : (
                  <button key={a.id} type="button" className="fbtn">
                    {a.label}
                  </button>
                ),
              )}
            </div>
            {v.actions.some((a) => a.locked) && (
              <div className="frow__d" style={{ marginTop: 10 }}>
                {tf.lockedNeedsKey}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="fcard">
        <div className="fcard__label">{tf.schedule}</div>
        {v.schedule.length === 0 && <Empty>{tf.emptySchedule}</Empty>}
        {v.schedule.map((s) => (
          <div className="frow" key={s.id}>
            <div className="frow__lead">
              <span className="frow__t">{s.title}</span>
              <span className="frow__d">
                {s.time} · {s.detail}
              </span>
            </div>
            <div className="frow__trail">
              <button type="button" className="fbtn">
                {tf.prep}
              </button>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

// ——— D4 · Sources ——————————————————————————————————————————————————————————————————————

export function Sources({ v }: { v: SourcesView }): JSX.Element {
  return (
    <>
      <div className="fcard">
        <div className="fcard__label">{tf.connectedServices}</div>
        <div className="fcard__sub">{tf.sourcesHint}</div>
        {v.sources.length === 0 && <Empty>{tf.emptySources}</Empty>}
        {v.sources.map((s) => (
          <div className="frow" key={s.id}>
            <div className="frow__lead">
              <span className="frow__t">{s.name}</span>
              <span className="frow__d">
                {s.scope} · {s.freshness}
              </span>
            </div>
            <div className="frow__trail">
              {s.third_party && <span className="pillbadge pillbadge--warn"><span className="pillbadge__dot" />{tf.thirdParty}</span>}
              <span className={`pillbadge${s.health === "ok" ? " pillbadge--ok" : " pillbadge--warn"}`}>
                <span className="pillbadge__dot" />
                {s.health === "ok" ? tf.healthy : tf.needsAttention}
              </span>
            </div>
          </div>
        ))}
      </div>

      <div className="fcard">
        <div className="fcard__label">{tf.exclusions}</div>
        <div className="fcard__sub">{tf.exclusionsHint}</div>
        {v.exclusions.map((e) => (
          <div className="frow" key={e.id}>
            <div className="frow__lead">
              <span className="frow__t">{e.title}</span>
              <span className="frow__d">{e.detail}</span>
            </div>
            <div className="frow__trail">
              {e.locked ? (
                <span className="pillbadge"><span className="pillbadge__dot" />{tf.alwaysExcluded}</span>
              ) : (
                <span className="pillbadge">{e.enabled ? tf.on : tf.off}</span>
              )}
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

// ——— D3 · Memory ———————————————————————————————————————————————————————————————————————

export function Memory({ v }: { v: MemoryView }): JSX.Element {
  return (
    <>
      <div className="fcard">
        <div className="fcard__label">{tf.commitments}</div>
        <div className="fcard__sub">{tf.commitmentsHint}</div>
        {v.commitments.length === 0 && <Empty>{tf.emptyCommitments}</Empty>}
        {v.commitments.map((r) => (
          <div className="frow" key={r.id}>
            <div className="frow__lead" style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
              <span className={`conf conf--${r.confidence}`}>{confLabel(r.confidence)}</span>
              <div style={{ display: "flex", flexDirection: "column", gap: 3, minWidth: 0 }}>
                <span className="frow__t" style={r.confidence === "low" ? { color: "var(--muted)" } : undefined}>
                  {/* Low-confidence rows are hedged, never stated as fact (data-model principle). */}
                  {r.confidence === "low" && <span style={{ color: "var(--faint)" }}>{tf.possibly} </span>}
                  {r.text}
                </span>
                <span className="frow__d">{r.detail}</span>
              </div>
            </div>
          </div>
        ))}
      </div>

      {v.merge_candidates.length > 0 && (
        <div className="fcard">
          <div className="fcard__label">{tf.needsYourEye}</div>
          <div className="fcard__sub">{tf.mergeHint}</div>
          {v.merge_candidates.map((m) => (
            <div className="frow" key={m.id}>
              <div className="frow__lead">
                <span className="frow__t">{m.names}</span>
                <span className="frow__d">{m.detail}</span>
              </div>
              <div className="frow__trail">
                <button type="button" className="fbtn">
                  {tf.keepSeparate}
                </button>
                <button type="button" className="fbtn fbtn--go">
                  {tf.merge}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

/** "Run now" for the nightly review — wired to the same command the panel's settings use. The
 *  view is a one-shot snapshot, so the row's numbers refresh on the next window open. */
function RunNowButton(): JSX.Element {
  const [busy, setBusy] = useState(false);
  return (
    <button
      type="button"
      className="fbtn"
      disabled={busy}
      onClick={() => {
        setBusy(true);
        void invoke("run_dream_now")
          .catch(() => undefined)
          .finally(() => setBusy(false));
      }}
    >
      {busy ? "…" : tf.runNow}
    </button>
  );
}

function confLabel(c: Confidence): string {
  return c === "high" ? tf.high : c === "medium" ? tf.medium : tf.low;
}

// ——— D5 · Activity —————————————————————————————————————————————————————————————————————

export function Activity({ v }: { v: ActivityView }): JSX.Element {
  return (
    <>
      {v.pending.length === 0 && (
        <div className="fcard">
          <div className="fcard__label">{tf.waitingForYou}</div>
          <Empty>{tf.emptyPending}</Empty>
        </div>
      )}
      {v.pending.map((p) => (
        <div className="fcard" key={p.id}>
          <div className="fcard__label">{tf.waitingForYou}</div>
          <div className="frow">
            <div className="frow__lead" style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
              <span className={`lvl lvl--${p.level.toLowerCase()}`}>{p.level}</span>
              <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                <span className="frow__t">{p.title}</span>
                <span className="frow__d">{p.detail}</span>
              </div>
            </div>
            <div className="frow__trail">
              {/* The confirm/reject controls are the panel's Approvals section — pointing there
                  beats a Review button with nothing behind it. */}
              <span className="frow__d">{tf.reviewInPanel}</span>
            </div>
          </div>
        </div>
      ))}

      <div className="fcard">
        <div className="fcard__label">{tf.runHistory}</div>
        {v.runs.length === 0 ? (
          // Not an empty table: on a plan without the agent engine there is nothing that could
          // have run, and saying so is more honest than a blank list.
          <div className="fcard__sub" style={{ paddingBottom: 18 }}>
            {tf.noRunsExplained}
          </div>
        ) : (
          <>
            <div className="thead">
              <span className="c-time">{tf.colTime}</span>
              <span className="c-grow">{tf.colAction}</span>
              <span className="c-mid">{tf.colApproved}</span>
              <span className="c-end">{tf.colLeft}</span>
            </div>
            {v.runs.map((r) => (
              <div className="trow" key={r.id}>
                <span className="c-time">{r.time}</span>
                <span className="c-grow">
                  <span className={`lvl lvl--${r.level.toLowerCase()}`} style={{ marginRight: 10 }}>
                    {r.level}
                  </span>
                  {r.action}
                </span>
                <span className="c-mid">{r.approved_by}</span>
                <span className="c-end">{r.egress ?? "—"}</span>
              </div>
            ))}
          </>
        )}
      </div>

      <div className="fcard">
        <div className="fcard__label">{tf.lastNightly}</div>
        <div className="frow">
          <div className="frow__lead">
            <span className="frow__t">
              {tf.finishedAt} {v.nightly.finished_at}
            </span>
            <span className="frow__d">
              {v.nightly.events_read.toLocaleString()} {tf.eventsRead} · {v.nightly.updates} {tf.updates} ·{" "}
              {v.nightly.chunks_sent} {tf.chunksSent}
            </span>
          </div>
          <div className="frow__trail">
            <span className={`pillbadge${v.nightly.health === "ok" ? " pillbadge--ok" : " pillbadge--warn"}`}>
              <span className="pillbadge__dot" />
              {v.nightly.health === "ok" ? tf.healthy : tf.needsAttention}
            </span>
            <RunNowButton />
          </div>
        </div>
      </div>
    </>
  );
}

// ——— D6 · Traceability —————————————————————————————————————————————————————————————————

export function Trace({ v }: { v: TraceView }): JSX.Element {
  return (
    <div className="fcard">
      <div className="fcard__label">{tf.everythingLeft}</div>
      {/* Say plainly that bodies are never logged — this screen is the privacy claim's receipt. */}
      <div className="fcard__sub">{tf.traceHint}</div>
      <div className="thead" style={{ marginTop: 14 }}>
        <span className="c-time">{tf.colTime}</span>
        <span className="c-mid">{tf.colRoute}</span>
        <span className="c-grow">{tf.colPurpose}</span>
        <span className="c-mid">{tf.colDestination}</span>
        <span className="c-grow">{tf.colDigest}</span>
        <span className="c-end">{tf.colBytes}</span>
      </div>
      {v.rows.length === 0 && <Empty>{tf.emptyTrace}</Empty>}
      {v.rows.map((r) => (
        <div className={`trow${r.route === "third_party" ? " trow--flag" : ""}`} key={r.id}>
          <span className="c-time">{r.time}</span>
          <span className="c-mid">{r.route === "third_party" ? tf.thirdParty : tf.direct}</span>
          <span className="c-grow">{r.purpose}</span>
          <span className="c-mid">{r.destination}</span>
          <span className="c-grow c-digest">{r.digest}</span>
          <span className="c-end">{r.bytes}</span>
        </div>
      ))}
      {v.third_party_count === 0 && (
        <div className="frow">
          <div className="frow__lead">
            <span className="frow__t" style={{ color: "var(--live)" }}>
              {tf.noThirdParty}
            </span>
            <span className="frow__d">{tf.noThirdPartyHint}</span>
          </div>
        </div>
      )}
    </div>
  );
}
