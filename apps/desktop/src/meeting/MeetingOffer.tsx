import type { JSX, PointerEventHandler, RefObject } from "react";

import meetingIcon from "../assets/meeting/shogun_meeting.svg";
import { t } from "../strings";

interface MeetingOfferProps {
  name: string;
  remainPct: number;
  progressRef: RefObject<HTMLDivElement>;
  onDrag: PointerEventHandler<HTMLDivElement>;
  onStart: () => void;
  onDismiss: () => void;
}

/** The host-only meeting offer surface. */
export function MeetingOffer({
  name,
  remainPct,
  progressRef,
  onDrag,
  onStart,
  onDismiss,
}: MeetingOfferProps): JSX.Element {
  return (
    <div className="ov-offer" onPointerDown={onDrag} title={t.meetingDisclosureBrief}>
      <div className="ov__offer ov__drag">
        <div className="ov__offer-row">
          <div className="ov__offer-left">
            <span className="ov__offer-accent" aria-hidden>
              {Array.from({ length: 5 }, (_, i) => (
                <span key={i} className="ov__offer-dot" />
              ))}
            </span>
            <div className="ov__offer-copy">
              <div className="ov__offer-title">{t.meetingDetected}</div>
              <div className="ov__offer-sub">{name}</div>
            </div>
          </div>
          <div className="ov__offer-acts ov__nodrag">
            <button type="button" className="ov__offer-go" onClick={onStart}>
              <img className="ov__offer-ico" src={meetingIcon} alt="" width={18} height={18} draggable={false} />
              {t.meetingTakeNotes}
            </button>
            <button type="button" className="ov__offer-skip" onClick={onDismiss}>
              {t.meetingNotNow}
            </button>
          </div>
        </div>
        <div
          ref={progressRef}
          className="ov__offer-progress"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={remainPct}
          aria-label={t.meetingStarting}
        />
      </div>
    </div>
  );
}
