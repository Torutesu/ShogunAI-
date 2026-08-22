// Daily summaries (issue #10, docs/daily-summaries-design.md §3): the Morning brief and the
// Evening wrap, rendered as a card that pours out of the notch. Everything completes here — no
// link out to a full view. Each line's chip re-opens the data source it came from.
//
// The webview draws what arrives (invariant 1): section content, times, hedges and chip labels
// are all composed on the Rust side (`daily_summaries.rs`); this file owns markup only.

import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MarkFacets, useMarkRefold } from "./Logo";
import { t } from "./strings";

const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// ── wire types (mirror src-tauri/src/daily_summaries.rs) ──────────────────────

export type SummaryWhich = "morning" | "evening";

export interface DailySettings {
  morning_enabled: boolean;
  evening_enabled: boolean;
  evening_hour: number;
  evening_minute: number;
}

export interface SummaryState {
  due: SummaryWhich | null;
  date: string;
  settings: DailySettings;
}

export interface SummaryLine {
  text: string;
  possibly: boolean;
  provenance_event_id: number;
  source: string | null;
}

export interface SummaryCalendarLine {
  time: string;
  title: string;
  updated: boolean;
}

export interface MorningView {
  generated: boolean;
  charm_line: string | null;
  today: SummaryCalendarLine[];
  commitments_due: SummaryLine[];
  open_loops: SummaryLine[];
  what_happened: string[];
}

export interface WrapView {
  outcome: {
    commitments_done: number;
    loops_closed: number;
    actions_decided: number;
    actions_adopted: number;
  };
  still_open: SummaryLine[];
  tomorrow_calendar: SummaryCalendarLine[];
  tomorrow_commitments: SummaryLine[];
  loose_ends: SummaryLine[];
}

// ── small pieces ──────────────────────────────────────────────────────────────

/** The confirmed mark in its native 957x614 space, tinted for dark glass. Refolds into the heart
 *  with the greeting row it sits in — the mark is 20px wide, far too small to aim at on its own. */
export function SummaryMark({ className }: { className?: string }): JSX.Element {
  const ref = useRef<SVGSVGElement>(null);
  useMarkRefold(ref, "heart");
  return (
    <svg ref={ref} viewBox="0 0 957 614" className={className ?? "scard__mark"} aria-hidden="true">
      <MarkFacets fill="currentColor" />
    </svg>
  );
}

/** "Fri, Aug 15" from the judgement's own local date string — presentation of a value the Rust
 *  side already fixed, never a fresh clock read (the card must show the day it was delivered for). */
function dateLabel(date: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!m) return date;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return d.toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" });
}

function openSource(eventId: number): void {
  if (!IN_TAURI) return;
  void invoke("open_summary_source", { eventId }).catch(() => undefined);
}

function Row({ line }: { line: SummaryLine }): JSX.Element {
  return (
    <div className={`scard__row${line.possibly ? " is-possibly" : ""}`}>
      <span className="scard__rowtext">{line.text}</span>
      {line.possibly ? <span className="scard__pill">{t.dsPossibly}</span> : null}
      {line.source ? (
        <button
          className="scard__src"
          type="button"
          title={line.source}
          onClick={() => openSource(line.provenance_event_id)}
        >
          {line.source}
        </button>
      ) : null}
    </div>
  );
}

function CalRow({ line }: { line: SummaryCalendarLine }): JSX.Element {
  return (
    <div className="scard__row">
      <span className="scard__time">{line.time}</span>
      <span className="scard__rowtext">{line.title}</span>
      {line.updated ? <span className="scard__pill is-updated">{t.dsUpdated}</span> : null}
    </div>
  );
}

function Section({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): JSX.Element | null {
  return (
    <div className="scard__sec">
      <div className="scard__label">{label}</div>
      {children}
    </div>
  );
}

function Greet({ which, date }: { which: SummaryWhich; date: string }): JSX.Element {
  return (
    <div className="scard__greet" data-mark-hover>
      <SummaryMark />
      <h2>{which === "morning" ? t.goodMorning : t.goodEvening}</h2>
      <span className="scard__date">{dateLabel(date)}</span>
    </div>
  );
}

// ── the card ──────────────────────────────────────────────────────────────────

/**
 * The delivered summary. Mounting IS the read receipt (§2: 既読 = カードを開いた): it calls
 * `mark_summary_seen` once, so the notice doesn't re-arm today — but the card itself stays
 * until dismissed.
 */
export function SummaryCard({
  which,
  date,
  onClose,
}: {
  which: SummaryWhich;
  date: string;
  onClose: () => void;
}): JSX.Element {
  const [morning, setMorning] = useState<MorningView | null>(null);
  const [wrap, setWrap] = useState<WrapView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke("mark_summary_seen", { which, date }).catch(() => undefined);
    if (which === "morning") {
      invoke<MorningView>("morning_card")
        .then(setMorning)
        .catch((e) => setError(String(e)));
    } else {
      invoke<WrapView>("evening_wrap")
        .then(setWrap)
        .catch((e) => setError(String(e)));
    }
  }, [which, date]);

  return (
    <div className="scard" role="region" aria-label={which === "morning" ? t.goodMorning : t.goodEvening}>
      <Greet which={which} date={date} />
      {error ? <div className="scard__empty">{error}</div> : null}
      {which === "morning" && morning ? <MorningBody v={morning} /> : null}
      {which === "evening" && wrap ? <EveningBody v={wrap} /> : null}
      <div className="scard__foot">
        <button className="chip" type="button" onClick={onClose}>
          {t.dsClose}
        </button>
      </div>
    </div>
  );
}

function MorningBody({ v }: { v: MorningView }): JSX.Element {
  const empty =
    v.today.length === 0 &&
    v.commitments_due.length === 0 &&
    v.open_loops.length === 0 &&
    v.what_happened.length === 0;
  return (
    <>
      {v.charm_line ? <p className="scard__charm">{v.charm_line}</p> : null}
      {v.today.length > 0 ? (
        <Section label={t.dsToday}>
          {v.today.map((l, i) => (
            <CalRow key={i} line={l} />
          ))}
        </Section>
      ) : null}
      {v.commitments_due.length > 0 ? (
        <Section label={t.dsCommitments}>
          {v.commitments_due.map((l) => (
            <Row key={l.provenance_event_id} line={l} />
          ))}
        </Section>
      ) : null}
      {v.open_loops.length > 0 ? (
        <Section label={t.dsOpenLoops}>
          {v.open_loops.map((l) => (
            <Row key={l.provenance_event_id} line={l} />
          ))}
        </Section>
      ) : null}
      {v.what_happened.length > 0 ? (
        <Section label={t.dsWhatHappened}>
          {v.what_happened.map((s, i) => (
            <div key={i} className="scard__row">
              <span className="scard__rowtext">{s}</span>
            </div>
          ))}
        </Section>
      ) : null}
      {empty ? <div className="scard__empty">{t.dsEmptyMorning}</div> : null}
    </>
  );
}

function EveningBody({ v }: { v: WrapView }): JSX.Element {
  return (
    <>
      <Section label={t.dsOutcome}>
        <div className="scard__outcome">
          <div className="scard__cell">
            <b className="is-good">{v.outcome.commitments_done}</b>
            <span>{t.dsDone}</span>
          </div>
          <div className="scard__cell">
            <b className="is-good">{v.outcome.loops_closed}</b>
            <span>{t.dsLoopsClosed}</span>
          </div>
          <div className="scard__cell">
            <b>
              {v.outcome.actions_adopted}/{v.outcome.actions_decided}
            </b>
            <span>{t.dsAdopted}</span>
          </div>
        </div>
      </Section>
      {v.still_open.length > 0 ? (
        <Section label={t.dsStillOpen}>
          {v.still_open.map((l) => (
            <Row key={l.provenance_event_id} line={l} />
          ))}
        </Section>
      ) : null}
      {v.tomorrow_calendar.length > 0 || v.tomorrow_commitments.length > 0 ? (
        <Section label={t.dsTomorrowFirst}>
          {v.tomorrow_calendar.map((l, i) => (
            <CalRow key={i} line={l} />
          ))}
          {v.tomorrow_commitments.map((l) => (
            <Row key={l.provenance_event_id} line={l} />
          ))}
        </Section>
      ) : null}
      {v.loose_ends.length > 0 ? (
        <Section label={t.dsLooseEnds}>
          {v.loose_ends.map((l) => (
            <Row key={l.provenance_event_id} line={l} />
          ))}
        </Section>
      ) : null}
    </>
  );
}
