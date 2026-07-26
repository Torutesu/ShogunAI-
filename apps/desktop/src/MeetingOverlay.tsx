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
  const [note, setNote] = useState("");

  useEffect(() => {
    const off = listen<MeetingView>("meeting", (e) => setView(e.payload));
    void invoke<MeetingView>("meeting_status").then(setView).catch(() => undefined);
    return () => void off.then((f) => f());
  }, []);

  // The Recap is read once the interval has closed — it is assembled from the stored session,
  // so there is nothing to show before then.
  useEffect(() => {
    if (view?.state !== "wrapping") return;
    void invoke<Recap | null>("meeting_recap").then(setRecap).catch(() => undefined);
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
        </div>
        <button type="button" className="ov__go ov__go--wide" onClick={() => call("meeting_wrapped")}>
          {t.meetingRecapDone}
        </button>
      </div>
    </div>
  );
}
