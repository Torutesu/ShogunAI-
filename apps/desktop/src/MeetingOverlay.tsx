// The meeting overlay: its own small floating window, parked top-right and draggable
// (Issue #7). Separate from the notch on purpose — during a meeting the user is looking at the
// meeting window, and the notch sits at the top edge of the screen outside that field of view.
// "Always visible, always one tap to stop" only holds if it appears near what they are watching.

import { useCallback, useEffect, useRef, useState, type JSX } from "react";
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

export interface Minutes {
  summary: string;
  decisions: string[];
  next_actions: { text: string; owner: string | null }[];
}

export interface TranscriptLine {
  ts: number;
  speaker: string | null;
  text: string;
  translation?: string | null;
}

interface MeetingTranscript {
  lines: TranscriptLine[];
  only_blanks: boolean;
}

type MeetingMode = "transcription" | "one_way" | "two_way";
type MeetingLanguage = "english" | "japanese" | "auto";

interface MeetingSettings {
  meeting_mode: MeetingMode;
  source_lang: MeetingLanguage;
  target_lang: MeetingLanguage;
  my_lang: MeetingLanguage;
  other_lang: MeetingLanguage;
}

interface TranscriptTurn {
  speakerLabel: string;
  ts: number;
  text: string;
  translation?: string | null;
}

interface LiveLineEvent {
  ts: number;
  speaker: string | null;
  text: string;
  translation?: string | null;
}

interface TranslationEvent {
  ts: number;
  translation: string;
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

function langLabel(lang: MeetingLanguage): string {
  if (lang === "auto") return t.meetingLangAuto;
  if (lang === "japanese") return t.meetingLangJapanese;
  return t.meetingLangEnglish;
}

function modeLabel(mode: MeetingMode): string {
  if (mode === "one_way") return t.meetingModeOneWay;
  if (mode === "two_way") return t.meetingModeTwoWay;
  return t.meetingModeTranscription;
}

function minutesHasContent(minutes: Minutes): boolean {
  return (
    minutes.summary.trim().length > 0 ||
    minutes.decisions.length > 0 ||
    minutes.next_actions.length > 0
  );
}

/** Drop LLM meta-chat/refusals — overlay must never paint these as subtitles. */
function looksLikeTranslateRefusal(text: string): boolean {
  const lower = text.toLowerCase().trim();
  if (!lower) return true;
  const needles = [
    "i don't see",
    "i do not see",
    "could you please",
    "please provide",
    "provide the",
    "spoken line",
    "audio content",
    "no text to translate",
    "nothing to translate",
    "you'd like translated",
    "can you provide",
  ];
  if (needles.some((n) => lower.includes(n))) return true;
  return ["sure,", "sure!", "certainly", "of course", "here is the translation", "here's the translation"].some(
    (p) => lower.startsWith(p),
  );
}

function usableTranslation(text: string | null | undefined): string | null {
  if (!text?.trim()) return null;
  if (looksLikeTranslateRefusal(text)) return null;
  return text.trim();
}

/** Group consecutive lines from the same speaker into readable turns. */
function groupTurns(lines: TranscriptLine[]): TranscriptTurn[] {
  if (lines.length === 0) return [];
  const origin = lines[0].ts;
  const turns: TranscriptTurn[] = [];
  for (const line of lines) {
    const label = speakerLabel(line.speaker);
    const last = turns[turns.length - 1];
    if (last && last.speakerLabel === label && last.translation === line.translation) {
      last.text = `${last.text} ${line.text}`;
    } else {
      turns.push({
        speakerLabel: label,
        ts: line.ts - origin,
        text: line.text,
        translation: line.translation,
      });
    }
  }
  return turns;
}

const call = (cmd: string, args?: Record<string, unknown>): void => {
  void invoke(cmd, args).catch(() => undefined);
};

/** AppKit hit-tests the whole NSWindow rect; CSS holes do not pass clicks to apps behind. */
function useOverlayInteractive(active: boolean): void {
  useEffect(() => {
    call("meeting_overlay_set_interactive", { interactive: active });
    return () => {
      call("meeting_overlay_set_interactive", { interactive: false });
    };
  }, [active]);
}

const startDrag = (): void => {
  call("meeting_drag");
};

/** Coalesce rapid `meeting_live_line` / translation events into one paint per frame. */
function useLiveLineBuffer(active: boolean): {
  lines: TranscriptLine[];
  pushLine: (line: TranscriptLine) => void;
  patchTranslation: (ts: number, translation: string) => void;
  snapshot: () => TranscriptLine[];
} {
  const [lines, setLines] = useState<TranscriptLine[]>([]);
  const pendingRef = useRef<TranscriptLine[]>([]);
  const transRef = useRef<Map<number, string>>(new Map());
  const rafRef = useRef<number | null>(null);

  const flush = useCallback((): void => {
    rafRef.current = null;
    const batch = pendingRef.current;
    const patches = transRef.current;
    pendingRef.current = [];
    transRef.current = new Map();
    if (batch.length === 0 && patches.size === 0) return;
    setLines((prev) => {
      let next = batch.length > 0 ? [...prev, ...batch] : prev;
      if (patches.size > 0) {
        next = next.map((l) => {
          const translation = patches.get(l.ts);
          return translation != null ? { ...l, translation } : l;
        });
      }
      return next;
    });
  }, []);

  const schedule = useCallback((): void => {
    if (rafRef.current != null) return;
    rafRef.current = window.requestAnimationFrame(flush);
  }, [flush]);

  useEffect(() => {
    if (active) {
      setLines([]);
      pendingRef.current = [];
      transRef.current = new Map();
      if (rafRef.current != null) {
        window.cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      return;
    }
    if (rafRef.current != null) {
      window.cancelAnimationFrame(rafRef.current);
      flush();
    }
  }, [active, flush]);

  useEffect(
    () => () => {
      if (rafRef.current != null) window.cancelAnimationFrame(rafRef.current);
    },
    [],
  );

  const pushLine = useCallback(
    (line: TranscriptLine) => {
      pendingRef.current.push(line);
      schedule();
    },
    [schedule],
  );

  const patchTranslation = useCallback(
    (ts: number, translation: string) => {
      const usable = usableTranslation(translation);
      if (!usable) return;
      transRef.current.set(ts, usable);
      schedule();
    },
    [schedule],
  );

  const snapshot = useCallback(
    (): TranscriptLine[] => [...lines, ...pendingRef.current],
    [lines],
  );

  return { lines, pushLine, patchTranslation, snapshot };
}

const MODES: MeetingMode[] = ["transcription", "one_way", "two_way"];
const LANGS: MeetingLanguage[] = ["auto", "english", "japanese"];
const ONE_WAY_TARGET_LANGS: MeetingLanguage[] = ["english", "japanese"];

export function MeetingOverlay(): JSX.Element | null {
  const [view, setView] = useState<MeetingView | null>(null);
  const [recap, setRecap] = useState<Recap | null>(null);
  const [minutes, setMinutes] = useState<Minutes | null>(null);
  const [transcript, setTranscript] = useState<TranscriptLine[]>([]);
  const [onlyBlanks, setOnlyBlanks] = useState(false);
  const [minutesNeedsKey, setMinutesNeedsKey] = useState(false);
  const [stopping, setStopping] = useState(false);
  const live = useLiveLineBuffer(view?.state === "recording");
  const liveLines = live.lines;
  const [settings, setSettings] = useState<MeetingSettings>({
    meeting_mode: "transcription",
    source_lang: "auto",
    target_lang: "japanese",
    my_lang: "english",
    other_lang: "japanese",
  });
  const [modeOpen, setModeOpen] = useState(false);
  const [langOpen, setLangOpen] = useState<"source" | "target" | "my" | "other" | null>(null);
  const [showSource, setShowSource] = useState(false);
  const liveScrollRef = useRef<HTMLDivElement>(null);
  const liveSnapshotRef = useRef<TranscriptLine[]>([]);

  useEffect(() => {
    liveSnapshotRef.current = live.snapshot();
  }, [liveLines, live.snapshot]);

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

  useEffect(() => {
    if (view?.state !== "recording") return;
    void invoke<MeetingSettings>("get_meeting_settings")
      .then(setSettings)
      .catch(() => undefined);

    const offLine = listen<LiveLineEvent>("meeting_live_line", (e) => {
      const line = e.payload;
      live.pushLine({
        ts: line.ts,
        speaker: line.speaker,
        text: line.text,
        translation: line.translation ?? null,
      });
    });
    const offTrans = listen<TranslationEvent>("meeting_live_translation", (e) => {
      const { ts, translation } = e.payload;
      live.patchTranslation(ts, translation);
    });
    return () => {
      void offLine.then((f) => f());
      void offTrans.then((f) => f());
    };
  }, [view?.state, live.pushLine, live.patchTranslation]);

  useEffect(() => {
    if (view?.state !== "recording") setStopping(false);
  }, [view?.state]);

  useEffect(() => {
    liveScrollRef.current?.scrollTo({
      top: liveScrollRef.current.scrollHeight,
      behavior: "smooth",
    });
  }, [liveLines.length]);

  useEffect(() => {
    if (view?.state !== "wrapping") return;
    setMinutesNeedsKey(false);
    setOnlyBlanks(false);
    // Carry live transcript into Recap so Stop does not flash an empty card.
    const seeded = liveSnapshotRef.current;
    if (seeded.length > 0) {
      setTranscript(seeded);
    }

    let transcriptDone = seeded.length > 0;
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

  const overlayActive = Boolean(view?.enabled && view && view.state !== "idle");
  useOverlayInteractive(overlayActive);

  if (!view || !view.enabled || view.state === "idle") return null;

  const name = view.title?.trim() || t.meetingUntitled;

  const grip = (
    <div
      className="ov__grip"
      title={t.meetingNotes}
      onPointerDown={(e) => {
        if (e.button === 0) startDrag();
      }}
    />
  );

  const handleStop = (): void => {
    if (stopping) return;
    setStopping(true);
    call("meeting_stop");
  };

  const setMode = (mode: MeetingMode): void => {
    setSettings((s) => ({ ...s, meeting_mode: mode }));
    setModeOpen(false);
    call("set_meeting_mode", { mode });
  };

  const setLang = (
    field: "source_lang" | "target_lang" | "my_lang" | "other_lang",
    lang: MeetingLanguage,
  ): void => {
    setSettings((s) => ({ ...s, [field]: lang }));
    setLangOpen(null);
    call("set_meeting_langs", { [field]: lang });
  };

  if (view.state === "offered") {
    return (
      <div className="ov ov--offer ov__nodrag">
        {grip}
        <div className="ov__body ov__drag">
          <div className="ov__kicker">{t.meetingDetected}</div>
          <div className="ov__name">{name}</div>
        </div>
        <div className="ov__acts ov__nodrag">
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
    const translating = settings.meeting_mode !== "transcription";
    const liveTurns = groupTurns(liveLines);

    const langPicker = (
      field: "source" | "target" | "my" | "other",
      langKey: "source_lang" | "target_lang" | "my_lang" | "other_lang",
      value: MeetingLanguage,
      options: MeetingLanguage[],
    ) => (
      <div className="ov__langpick">
        <button
          type="button"
          className={`ov__langbtn${field === "source" || field === "my" ? " ov__langbtn--auto" : ""}`}
          onClick={() => setLangOpen(langOpen === field ? null : field)}
        >
          {langLabel(value)}
        </button>
        {langOpen === field ? (
          <div className="ov__langmenu" role="listbox">
            {options.map((opt) => (
              <button
                key={opt}
                type="button"
                role="option"
                aria-selected={opt === value}
                className={`ov__langopt${opt === value ? " is-on" : ""}`}
                onClick={() => setLang(langKey, opt)}
              >
                {langLabel(opt)}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    );

    return (
      <div className="ov ov--live ov__nodrag">
        {grip}
        <div className="ov__live">
          <header className="ov__livehead ov__drag">
            <div className="ov__modepick">
              <button
                type="button"
                className="ov__modebtn"
                aria-expanded={modeOpen}
                onClick={() => {
                  setModeOpen(!modeOpen);
                  setLangOpen(null);
                }}
              >
                {modeLabel(settings.meeting_mode)}
                <span className="ov__chev" aria-hidden />
              </button>
              {modeOpen ? (
                <div className="ov__modemenu" role="listbox">
                  {MODES.map((m) => (
                    <button
                      key={m}
                      type="button"
                      role="option"
                      aria-selected={m === settings.meeting_mode}
                      className={`ov__modeopt${m === settings.meeting_mode ? " is-on" : ""}`}
                      onClick={() => setMode(m)}
                    >
                      {modeLabel(m)}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>

            {settings.meeting_mode === "one_way" ? (
              <div className="ov__langrow">
                {langPicker("source", "source_lang", settings.source_lang, LANGS)}
                <span className="ov__langarrow">{t.meetingLangArrow}</span>
                {langPicker("target", "target_lang", settings.target_lang, ONE_WAY_TARGET_LANGS)}
              </div>
            ) : null}

            {settings.meeting_mode === "two_way" ? (
              <div className="ov__langrow">
                {langPicker("my", "my_lang", settings.my_lang, ONE_WAY_TARGET_LANGS)}
                <span className="ov__langarrow">{t.meetingLangSwap}</span>
                {langPicker("other", "other_lang", settings.other_lang, ONE_WAY_TARGET_LANGS)}
              </div>
            ) : null}

            <div className="ov__liveacts ov__nodrag">
              {translating ? (
                <button
                  type="button"
                  className="ov__iconbtn"
                  title={showSource ? t.meetingLiveHideSource : t.meetingLiveShowSource}
                  aria-label={showSource ? t.meetingLiveHideSource : t.meetingLiveShowSource}
                  onClick={() => setShowSource(!showSource)}
                >
                  <svg className="ov__icon" viewBox="0 0 24 24" aria-hidden>
                    <path
                      d="M4 8h16M6 12h12M8 16h8"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.5"
                      strokeLinecap="round"
                    />
                  </svg>
                </button>
              ) : null}
              <button
                type="button"
                className="ov__iconbtn"
                title={t.meetingOverlayClose}
                aria-label={t.meetingOverlayClose}
                onClick={() => call("meeting_overlay_dismiss")}
              >
                <svg className="ov__icon" viewBox="0 0 24 24" aria-hidden>
                  <path
                    d="M6 6l12 12M18 6L6 18"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            </div>
          </header>

          <div className="ov__livebody ov__nodrag" ref={liveScrollRef}>
            {liveTurns.length === 0 ? (
              <p className="ov__liveempty">{t.meetingLiveEmpty}</p>
            ) : (
              liveTurns.map((turn, i) => {
                const translation = usableTranslation(turn.translation);
                const primary = translating && translation ? translation : turn.text;
                const secondary =
                  translating && translation && showSource ? turn.text : null;
                return (
                  <div className="ov__liveline" key={`${turn.ts}-${i}`}>
                    <div className="ov__livemeta">
                      <span className="ov__livespeaker">{turn.speakerLabel}</span>
                      <span className="ov__livetime">{clock(turn.ts)}</span>
                    </div>
                    <p className="ov__livetext">{primary}</p>
                    {secondary ? <p className="ov__livesrc">{secondary}</p> : null}
                  </div>
                );
              })
            )}
          </div>

          <footer className="ov__livefoot ov__drag">
            <span className="ov__livetitle">{name}</span>
            <span className="ov__livetime ov__livetime--foot">{clock(view.elapsed_ms)}</span>
            <button
              type="button"
              className="ov__stop ov__stop--live ov__nodrag"
              disabled={stopping}
              onClick={handleStop}
            >
              <span className="ov__stopdot" />
              {t.meetingStop}
            </button>
          </footer>
        </div>
      </div>
    );
  }

  const minutesReady = minutes != null && minutesHasContent(minutes);
  const turns = groupTurns(transcript);
  const hasTranscript = turns.length > 0;
  const showMinutesPending = hasTranscript && !minutesReady && !minutesNeedsKey;
  const showMinutesNeedsKey = hasTranscript && !minutesReady && minutesNeedsKey;
  const showWrapping = !recap && !minutesReady && !hasTranscript;

  return (
    <div className="ov ov--recap ov__nodrag">
      {grip}
      <div className="ov__recap">
        <div className="ov__rhead ov__drag">
          <span className="ov__rtitle">{recap?.title ?? name}</span>
          {recap?.duration_minutes != null ? (
            <span className="ov__rmin">
              {recap.duration_minutes} {t.meetingRecapMinutes}
            </span>
          ) : null}
        </div>
        <div className="ov__rbody ov__nodrag">
          {showWrapping ? <p className="ov__mpending">{t.meetingMinutesPending}</p> : null}
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
        <button type="button" className="ov__go ov__go--wide ov__nodrag" onClick={() => call("meeting_wrapped")}>
          {t.meetingRecapDone}
        </button>
      </div>
    </div>
  );
}
