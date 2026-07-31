// The meeting overlay: its own small floating window, parked top-right and draggable
// (Issue #7). Separate from the notch on purpose — during a meeting the user is looking at the
// meeting window, and the notch sits at the top edge of the screen outside that field of view.
// "Always visible, always one tap to stop" only holds if it appears near what they are watching.

import { useEffect, useState, type JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { t } from "./strings";
import { uiLog } from "./uiLog";

export interface MeetingView {
  state: "idle" | "offered" | "recording" | "wrapping";
  enabled: boolean;
  title: string | null;
  app_bundle_id: string | null;
  elapsed_ms: number;
  countdown_ms: number;
}

export interface Recap {
  title: string;
  duration_minutes: number | null;
  notes: string | null;
  captured_events: number;
  degraded: boolean;
}

// The model-generated minutes (MT4). They arrive AFTER the degraded Recap is already on screen —
// the Batch lane is async — so this is layered on top of `Recap`, never a replacement. A next
// action is a suggestion to confirm, not something the app will do (invariant 4): display only.
export interface Minutes {
  summary: string;
  decisions: string[];
  next_actions: { text: string; owner: string | null }[];
}

export interface TranscriptLine {
  ts: number;
  speaker: string | null;
  text: string;
}

interface MeetingTranscript {
  lines: TranscriptLine[];
  only_blanks: boolean;
}

interface TranscriptTurn {
  speakerLabel: string;
  ts: number;
  text: string;
}

/** mm:ss. Tabular figures in CSS keep the row from reflowing as the seconds tick. */
function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

function speakerLabel(speaker: string | null): string {
  if (speaker === "me") return t.meetingTranscriptSpeakerMe;
  if (speaker === "other") return t.meetingTranscriptSpeakerOther;
  return t.meetingTranscriptSpeakerUnknown;
}

function minutesHasContent(minutes: Minutes): boolean {
  return (
    minutes.summary.trim().length > 0 ||
    minutes.decisions.length > 0 ||
    minutes.next_actions.length > 0
  );
}

/** Group consecutive lines from the same speaker into readable turns. */
function groupTurns(lines: TranscriptLine[]): TranscriptTurn[] {
  if (lines.length === 0) return [];
  const origin = lines[0].ts;
  const turns: TranscriptTurn[] = [];
  for (const line of lines) {
    const label = speakerLabel(line.speaker);
    const last = turns[turns.length - 1];
    if (last && last.speakerLabel === label) {
      last.text = `${last.text} ${line.text}`;
    } else {
      turns.push({ speakerLabel: label, ts: line.ts - origin, text: line.text });
    }
  }
  return turns;
}

const call = (cmd: string, args?: Record<string, unknown>): void => {
  void invoke(cmd, args).catch(() => undefined);
};

export function MeetingOverlay(): JSX.Element | null {
  const [view, setView] = useState<MeetingView | null>(null);
  const [recap, setRecap] = useState<Recap | null>(null);
  const [minutes, setMinutes] = useState<Minutes | null>(null);
  const [transcript, setTranscript] = useState<TranscriptLine[]>([]);
  const [onlyBlanks, setOnlyBlanks] = useState(false);
  const [minutesNeedsKey, setMinutesNeedsKey] = useState(false);
  const [note, setNote] = useState("");

  // Polled, with the push event as an accelerator rather than the source of truth.
  //
  // Relying on the event alone left this window holding the state it happened to see when it
  // mounted: it rendered "idle", never heard another word, and stayed blank while a meeting ran
  // — a transparent window with nothing in it is indistinguishable from no window at all. A
  // status call a second is nothing next to a surface that has to be right whenever the user
  // looks at it.
  useEffect(() => {
    const read = (): void => {
      void invoke<MeetingView>("meeting_status").then(setView).catch(() => undefined);
    };
    read();
    const timer = window.setInterval(read, 1000);
    const off = listen<MeetingView>("meeting", (e) => setView(e.payload));
    return () => {
      window.clearInterval(timer);
      void off.then((f) => f());
    };
  }, []);

  // Recap reads: degraded card, minutes (async), transcript. Transcript is fetched on wrap and
  // polled until lines land — the audio lane flushes on Stop, but a one-shot fetch raced empty
  // before segments were visible, and the meeting window lacked IPC permission until fixed.
  useEffect(() => {
    if (view?.state !== "wrapping") return;
    setMinutesNeedsKey(false);
    setTranscript([]);
    setOnlyBlanks(false);

    let transcriptDone = false;
    let minutesDone = false;

    void invoke<boolean>("meeting_select_kk_configured")
      .then((configured) => {
        if (!configured) setMinutesNeedsKey(true);
      })
      .catch(() => undefined);

    const fetchRecap = (): void => {
      void invoke<Recap | null>("meeting_recap")
        .then(setRecap)
        .catch((err) => uiLog(`meeting overlay invoke meeting_recap failed: ${err}`));
    };
    const fetchMinutes = (): void => {
      if (minutesDone) return;
      void invoke<Minutes | null>("meeting_recap_minutes")
        .then((m) => {
          setMinutes(m);
          if (m && minutesHasContent(m)) minutesDone = true;
        })
        .catch((err) => uiLog(`meeting overlay invoke meeting_recap_minutes failed: ${err}`));
    };
    const fetchTranscript = (): void => {
      if (transcriptDone) return;
      void invoke<MeetingTranscript>("get_meeting_transcript")
        .then((res) => {
          setTranscript(res.lines);
          setOnlyBlanks(res.only_blanks);
          if (res.lines.length > 0 || res.only_blanks) transcriptDone = true;
        })
        .catch((err) => uiLog(`meeting overlay invoke get_meeting_transcript failed: ${err}`));
    };

    fetchRecap();
    fetchMinutes();
    fetchTranscript();

    const poll = window.setInterval(() => {
      fetchTranscript();
      fetchMinutes();
    }, 1000);

    const offRecap = listen("meeting_recap", () => {
      fetchMinutes();
      fetchTranscript();
    });
    const offNeedsKey = listen("meeting_recap_needs_key", () => setMinutesNeedsKey(true));
    return () => {
      window.clearInterval(poll);
      void offRecap.then((f) => f());
      void offNeedsKey.then((f) => f());
    };
  }, [view?.state]);

  if (!view || !view.enabled || view.state === "idle") return null;

  const name = view.title?.trim() || t.meetingUntitled;

  // A grip strip down the left edge of every state: the window has no title bar, so this is the
  // only affordance that says "you can move me".
  const grip = (
    <div className="ov__grip" onMouseDown={() => call("meeting_drag")} title={t.meetingNotes} />
  );

  if (view.state === "offered") {
    return (
      <div className="ov ov--offer">
        {grip}
        <div className="ov__body">
          <div className="ov__kicker">{t.meetingDetected}</div>
          <div className="ov__name">{name}</div>
        </div>
        <div className="ov__acts">
          {/* One primary action, as in the reference. The countdown lives inside the button so
              the user sees the deadline without it becoming a second thing to read. */}
          <button type="button" className="ov__go" onClick={() => call("meeting_start")}>
            {t.meetingTakeNotes}
            <span className="ov__count">{Math.ceil(view.countdown_ms / 1000)}</span>
          </button>
          <button type="button" className="ov__quiet" onClick={() => call("meeting_not_now")}>
            {t.meetingNotNow}
          </button>
        </div>
        <p className="ov__disclosure">{t.meetingDisclosureBrief}</p>
      </div>
    );
  }

  if (view.state === "recording") {
    return (
      <div className="ov">
        {grip}
        <div className="ov__body">
          <div className="ov__time">{clock(view.elapsed_ms)}</div>
          <div className="ov__name ov__name--sub">{name}</div>
          <div className="ov__listening">{t.meetingListening}</div>
        </div>
        <input
          className="ov__note"
          value={note}
          placeholder={t.meetingNotePlaceholder}
          onChange={(e) => setNote(e.target.value)}
          onBlur={() => note && call("meeting_save_note", { body: note })}
          onKeyDown={(e) => {
            if (e.key === "Enter" && note) {
              call("meeting_save_note", { body: note });
            }
          }}
        />
        {/* Stop is the largest target here and takes no confirmation: stopping must never be
            harder than starting was. */}
        <button type="button" className="ov__stop" onClick={() => call("meeting_stop")}>
          <span className="ov__stopdot" />
          {t.meetingStop}
        </button>
      </div>
    );
  }

  // Wrapping: the degraded Recap (FR-MT-19). The summary and the action items arrive with MT4;
  // until then this shows what is actually known, and says so rather than leaving a blank card
  // that reads as "your meeting was lost". The transcript is shown here only — never during
  // recording (FR-MT-10).
  const minutesReady = minutes != null && minutesHasContent(minutes);
  const turns = groupTurns(transcript);
  const hasTranscript = turns.length > 0;
  const showMinutesPending = hasTranscript && !minutesReady && !minutesNeedsKey;
  const showMinutesNeedsKey = hasTranscript && !minutesReady && minutesNeedsKey;

  return (
    <div className="ov ov--recap">
      {grip}
      <div className="ov__recap">
        <div className="ov__rhead">
          <span className="ov__rtitle">{recap?.title ?? name}</span>
          {recap?.duration_minutes != null ? (
            <span className="ov__rmin">
              {recap.duration_minutes} {t.meetingRecapMinutes}
            </span>
          ) : null}
        </div>
        <div className="ov__rbody">
          {minutesReady ? (
            <div className="ov__minutes ov__minutes--lead">
              {minutes?.summary ? (
                <div className="ov__msec">
                  <div className="ov__mhead">{t.meetingMinutesSummary}</div>
                  <p className="ov__msummary">{minutes.summary}</p>
                </div>
              ) : null}
              {minutes && minutes.decisions.length > 0 ? (
                <div className="ov__msec">
                  <div className="ov__mhead">{t.meetingMinutesDecisions}</div>
                  <ul className="ov__mlist">
                    {minutes.decisions.map((d, i) => (
                      <li key={i}>{d}</li>
                    ))}
                  </ul>
                </div>
              ) : null}
              {minutes && minutes.next_actions.length > 0 ? (
                <div className="ov__msec">
                  <div className="ov__mhead">{t.meetingMinutesNextActions}</div>
                  <ul className="ov__mlist">
                    {minutes.next_actions.map((a, i) => (
                      <li key={i}>
                        {a.text}
                        {a.owner ? <span className="ov__mowner">{a.owner}</span> : null}
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
            </div>
          ) : null}
          {showMinutesPending ? (
            <p className="ov__mpending">{t.meetingMinutesPending}</p>
          ) : null}
          {showMinutesNeedsKey ? (
            <p className="ov__mdegraded">{t.meetingMinutesNeedsKey}</p>
          ) : null}
          {recap?.notes ? (
            <div className="ov__notes">
              <div className="ov__mhead">{t.meetingRecapYourNotes}</div>
              <pre className="ov__rnotes">{recap.notes}</pre>
            </div>
          ) : !minutesReady && !hasTranscript ? (
            <div className="ov__rempty">{t.meetingRecapNoNotes}</div>
          ) : null}
          <div className="ov__tsec">
            <div className="ov__mhead">{t.meetingTranscriptHeading}</div>
            {hasTranscript ? (
              <div className="ov__tturns">
                {turns.map((turn, i) => (
                  <div className="ov__tturn" key={i}>
                    <div className="ov__tmeta">
                      <span className="ov__tspeaker">{turn.speakerLabel}</span>
                      <span className="ov__ttime">{clock(turn.ts)}</span>
                    </div>
                    <p className="ov__ttext">{turn.text}</p>
                  </div>
                ))}
              </div>
            ) : onlyBlanks ? (
              <p className="ov__tempty">{t.meetingTranscriptOnlyBlanks}</p>
            ) : (
              <p className="ov__tempty">{t.meetingTranscriptEmpty}</p>
            )}
          </div>
        </div>
        <p className="ov__disclosure">{t.meetingDisclosureRecap}</p>
        <button type="button" className="ov__go ov__go--wide" onClick={() => call("meeting_wrapped")}>
          {t.meetingRecapDone}
        </button>
      </div>
    </div>
  );
}
