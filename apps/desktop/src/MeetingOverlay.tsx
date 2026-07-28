// The meeting overlay: its own small floating window, parked top-right and draggable
// (Issue #7). Separate from the notch on purpose — during a meeting the user is looking at the
// meeting window, and the notch sits at the top edge of the screen outside that field of view.
// "Always visible, always one tap to stop" only holds if it appears near what they are watching.

import { useEffect, useState, type JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { t } from "./strings";

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

/** mm:ss. Tabular figures in CSS keep the row from reflowing as the seconds tick. */
function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

const call = (cmd: string, args?: Record<string, unknown>): void => {
  void invoke(cmd, args).catch(() => undefined);
};

export function MeetingOverlay(): JSX.Element | null {
  const [view, setView] = useState<MeetingView | null>(null);
  const [recap, setRecap] = useState<Recap | null>(null);
  const [minutes, setMinutes] = useState<Minutes | null>(null);
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

  // The Recap is read once the interval has closed — it is assembled from the stored session,
  // so there is nothing to show before then. The model-generated minutes are read alongside it,
  // but they usually are not ready yet (the Batch lane is async): the `meeting_recap` event, fired
  // when they land, is what triggers the refetch that fills them in on the card already shown.
  useEffect(() => {
    if (view?.state !== "wrapping") return;
    void invoke<Recap | null>("meeting_recap").then(setRecap).catch(() => undefined);
    void invoke<Minutes | null>("meeting_recap_minutes").then(setMinutes).catch(() => undefined);
    const off = listen("meeting_recap", () => {
      void invoke<Minutes | null>("meeting_recap_minutes").then(setMinutes).catch(() => undefined);
    });
    return () => {
      void off.then((f) => f());
    };
  }, [view?.state]);

  // Why nothing is on screen, when nothing is on screen. The window can be shown and still look
  // empty, and from the outside that is indistinguishable from the window never appearing.
  useEffect(() => {
    call("ui_log", {
      msg: `overlay render view=${view ? `${view.state}/enabled=${view.enabled}` : "null"}`,
    });
  }, [view?.state, view?.enabled]);

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
  // that reads as "your meeting was lost".
  const minutesHasContent =
    minutes != null &&
    (minutes.summary.trim().length > 0 ||
      minutes.decisions.length > 0 ||
      minutes.next_actions.length > 0);
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
          {recap?.notes ? (
            <pre className="ov__rnotes">{recap.notes}</pre>
          ) : (
            <div className="ov__rempty">{t.meetingRecapNoNotes}</div>
          )}
          {/* The model-generated minutes, layered on top of the degraded Recap when they arrive.
              Never blanks the card while absent: the notes above always show. Display only — a
              next action is a suggestion to confirm, not something this system will do. */}
          {minutesHasContent ? (
            <div className="ov__minutes">
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
        </div>
        <button type="button" className="ov__go ov__go--wide" onClick={() => call("meeting_wrapped")}>
          {t.meetingRecapDone}
        </button>
      </div>
    </div>
  );
}
