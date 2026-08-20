import type { JSX, PointerEventHandler } from "react";

import { DragHandle6Dot } from "../DragHandle6Dot";
import { t } from "../strings";
import { clock, minutesHasContent, type Minutes, type TranscriptTurn } from "./transforms";
import type { Recap } from "./overlayTypes";

interface MeetingRecapProps {
  name: string;
  recap: Recap | null;
  minutes: Minutes | null;
  turns: TranscriptTurn[];
  onlyBlanks: boolean;
  minutesNeedsKey: boolean;
  onDrag: PointerEventHandler<HTMLDivElement>;
  onWrapped: () => void;
}

/** The host-only wrapping surface. Data loading stays with the overlay controller. */
export function MeetingRecap({
  name,
  recap,
  minutes,
  turns,
  onlyBlanks,
  minutesNeedsKey,
  onDrag,
  onWrapped,
}: MeetingRecapProps): JSX.Element {
  const minutesReady = minutes != null && minutesHasContent(minutes);
  const hasTranscript = turns.length > 0;
  const showMinutesPending = hasTranscript && !minutesReady && !minutesNeedsKey;
  const showMinutesNeedsKey = hasTranscript && !minutesReady && minutesNeedsKey;
  const showWrapping = !recap && !minutesReady && !hasTranscript;

  return (
    <div className="ov ov--recap" onPointerDown={onDrag}>
      <div className="ov__recap">
        <header className="ov__rhead">
          <div className="ov__rhead-main ov__drag" onPointerDown={onDrag}>
            <div className="ov__rkicker">{t.meetingRecapTitle}</div>
            <div className="ov__rtitle">{recap?.title ?? name}</div>
          </div>
          <div className="ov__rhead-tools">
            {recap?.duration_minutes != null ? (
              <span className="ov__rmin">{recap.duration_minutes} {t.meetingRecapMinutes}</span>
            ) : null}
            <DragHandle6Dot className="ov__drag ov__panel-grip" title={t.meetingNotes} onPointerDown={onDrag} />
          </div>
        </header>

        <div className="ov__rbody ov__nodrag">
          {showWrapping ? <Status>{t.meetingMinutesPending}</Status> : null}
          {minutesReady ? <MinutesContent minutes={minutes} /> : null}
          {showMinutesPending ? <Status>{t.meetingMinutesPending}</Status> : null}
          {showMinutesNeedsKey ? <p className="ov__mdegraded">{t.meetingMinutesNeedsKey}</p> : null}

          {recap?.notes ? (
            <section className="ov__notes">
              <div className="ov__mhead">{t.meetingRecapYourNotes}</div>
              <pre className="ov__rnotes">{recap.notes}</pre>
            </section>
          ) : !minutesReady && !hasTranscript ? <div className="ov__rempty">{t.meetingRecapNoNotes}</div> : null}

          <section className="ov__tsec">
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
            ) : onlyBlanks ? <p className="ov__tempty">{t.meetingTranscriptOnlyBlanks}</p> : <p className="ov__tempty">{t.meetingTranscriptEmpty}</p>}
          </section>
        </div>

        <footer className="ov__rfoot ov__nodrag">
          <p className="ov__disclosure">{t.meetingDisclosureRecap}</p>
          <button type="button" className="ov__rdone" onClick={onWrapped}>{t.meetingRecapDone}</button>
        </footer>
      </div>
    </div>
  );
}

function Status({ children }: { children: string }): JSX.Element {
  return <div className="ov__mstatus"><span className="ov__mstatus-dot" aria-hidden /><p className="ov__mpending">{children}</p></div>;
}

function MinutesContent({ minutes }: { minutes: Minutes }): JSX.Element {
  return (
    <div className="ov__minutes ov__minutes--lead">
      {minutes.summary ? <section className="ov__msec ov__msec--summary"><div className="ov__mhead">{t.meetingMinutesSummary}</div><p className="ov__msummary">{minutes.summary}</p></section> : null}
      {minutes.decisions.length > 0 ? <section className="ov__msec"><div className="ov__mhead">{t.meetingMinutesDecisions}</div><ul className="ov__mlist">{minutes.decisions.map((decision, i) => <li key={i}>{decision}</li>)}</ul></section> : null}
      {minutes.next_actions.length > 0 ? <section className="ov__msec"><div className="ov__mhead">{t.meetingMinutesNextActions}</div><ul className="ov__mlist ov__mlist--actions">{minutes.next_actions.map((action, i) => <li key={i}><span className="ov__maction">{action.text}</span>{action.owner ? <span className="ov__mowner">{action.owner}</span> : null}</li>)}</ul></section> : null}
    </div>
  );
}
