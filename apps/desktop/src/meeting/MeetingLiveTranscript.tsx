import type { JSX, RefObject } from "react";

import { t } from "../strings";
import { usableTranslation, type TranscriptTurn } from "./transforms";
import type { CaptionSplit, MeetingSettings } from "./overlayTypes";

interface MeetingLiveTranscriptProps {
  turns: TranscriptTurn[];
  settings: MeetingSettings;
  translating: boolean;
  split: CaptionSplit;
  showOriginal: boolean;
  translateKeyIssue: "missing" | "invalid" | null;
  scrollRef: RefObject<HTMLDivElement>;
}

/** Captions content only. Header controls and all Tauri state stay with the overlay controller. */
export function MeetingLiveTranscript({
  turns,
  settings,
  translating,
  split,
  showOriginal,
  translateKeyIssue,
  scrollRef,
}: MeetingLiveTranscriptProps): JSX.Element {
  const hideSource = settings.meeting_mode === "one_way" && !showOriginal;

  return (
    <div className={`ov__livebody ov__nodrag${translating ? " ov__livebody--split" : ""}`} data-no-drag ref={scrollRef}>
      {translateKeyIssue && translating ? <p className="ov__mdegraded ov__mdegraded--warn">{translateKeyIssue === "invalid" ? t.meetingTranslateKeyInvalid : t.meetingTranslateNeedsKey}</p> : null}
      {turns.length === 0 ? <p className="ov__liveempty">{t.meetingLiveEmpty}</p> : translating && split === "stack" ? <StackedTurns turns={turns} hideSource={hideSource} /> : translating ? <SplitTurns turns={turns} hideSource={hideSource} /> : <div className="ov__transcript">{turns.map((turn, i) => <p className="ov__transcript-line" key={`${turn.ts}-${i}`}>{turn.text}</p>)}</div>}
    </div>
  );
}

function StackedTurns({ turns, hideSource }: { turns: TranscriptTurn[]; hideSource: boolean }): JSX.Element {
  return <div className={`ov__stack${hideSource ? " ov__stack--dst-only" : ""}`}>{turns.map((turn, i) => {
    const translation = usableTranslation(turn.translation);
    return <div className="ov__stack-turn" key={`stack-${turn.ts}-${i}`}>{!hideSource ? <p className="ov__stack-src">{turn.text}</p> : null}<p className={`ov__stack-dst${translation ? "" : " is-pending"}`} aria-busy={!translation}>{translation ?? ""}</p></div>;
  })}</div>;
}

function SplitTurns({ turns, hideSource }: { turns: TranscriptTurn[]; hideSource: boolean }): JSX.Element {
  return <div className={`ov__split${hideSource ? " ov__split--dst-only" : ""}`}>{!hideSource ? <div className="ov__split-col ov__split-col--src">{turns.map((turn, i) => <p className="ov__split-line" key={`src-${turn.ts}-${i}`}>{turn.text}</p>)}</div> : null}{!hideSource ? <div className="ov__split-div" aria-hidden /> : null}<div className="ov__split-col ov__split-col--dst">{turns.map((turn, i) => {
    const translation = usableTranslation(turn.translation);
    return <p className={`ov__split-line${translation ? "" : " is-pending"}`} key={`dst-${turn.ts}-${i}`} aria-busy={!translation}>{translation ?? ""}</p>;
  })}</div></div>;
}
