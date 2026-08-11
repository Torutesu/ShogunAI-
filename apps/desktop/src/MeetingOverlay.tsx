// The meeting overlay: its own small floating window (Issue #7). Rust parks the offer card
// top-right and the in-meeting control pill bottom-center above the Meet mic bar. Draggable.

import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { flushSync } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import meetingIcon from "./assets/meeting/shogun_meeting.svg";
import notesIcon from "./assets/meeting/notes.svg";
import ccIcon from "./assets/meeting/cc.svg";
import chatIcon from "./assets/meeting/chat.svg";
import moreIcon from "./assets/meeting/more.svg";
import stopIcon from "./assets/meeting/stop.svg";
import closeIcon from "./assets/meeting/close.svg";
import preferenceIcon from "./assets/meeting/preference.svg";
import { DragHandle6Dot } from "./DragHandle6Dot";
import { ResizeCornerHandle } from "./ResizeCornerHandle";
import { t } from "./strings";
import { uiLog } from "./uiLog";
import { IconCopy } from "./utilityIcons";

/** Must match `Params::default().offer_grace_ms` in shogun-core meeting statemachine. */
const OFFER_GRACE_MS = 10_000;

export interface MeetingView {
  state: "idle" | "offered" | "recording" | "wrapping";
  enabled: boolean;
  title: string | null;
  app_bundle_id: string | null;
  elapsed_ms: number;
  countdown_ms: number;
  /** Capture/ASR paused; meeting session still open (waveform toggle). */
  paused: boolean;
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
  interim?: boolean;
}

interface TranslationEvent {
  ts: number;
  speaker?: string | null;
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

type CanvasMode = "live_summary" | "timeline";
type CaptionTextSize = "s" | "m" | "l";
type CaptionTextWeight = "light" | "bold";
type CaptionSplit = "side" | "stack";

interface ChatMessage {
  role: "user" | "assistant";
  text: string;
}

interface TimelineStep {
  ts: number;
  label: string;
  detail: string;
}

/** Chronological milestone list — time-bucketed speaker clusters. */
function buildTimeline(lines: TranscriptLine[]): TimelineStep[] {
  const turns = groupTurns(lines);
  if (turns.length === 0) return [];
  const steps: TimelineStep[] = [];
  let bucket: TranscriptTurn[] = [];
  let bucketStart = 0;
  const flush = (): void => {
    if (bucket.length === 0) return;
    const speakers = [...new Set(bucket.map((b) => b.speakerLabel))];
    const detail = bucket.map((b) => b.text.trim()).filter(Boolean).join(" ");
    steps.push({
      ts: bucket[0].ts,
      label: speakers.join(" · "),
      detail: detail.length > 200 ? `${detail.slice(0, 197)}…` : detail,
    });
    bucket = [];
  };
  for (const turn of turns) {
    if (bucket.length === 0) {
      bucket = [turn];
      bucketStart = turn.ts;
      continue;
    }
    if (bucket.length >= 4 || turn.ts - bucketStart > 90_000) {
      flush();
      bucket = [turn];
      bucketStart = turn.ts;
    } else {
      bucket.push(turn);
    }
  }
  flush();
  return steps;
}

const call = (cmd: string, args?: Record<string, unknown>): void => {
  void invoke(cmd, args).catch(() => undefined);
};

/** Drag the current meeting overlay window. Pass the window label so panel windows drag themselves. */
function beginMeetingDrag(e: React.PointerEvent, windowLabel?: string): void {
  if (e.button !== 0) return;
  const el = e.target as HTMLElement;
  if (el.closest("button, input, a, textarea, select, [data-no-drag]")) return;
  if (
    el.closest(
      ".ov__livebody, .ov__rbody, .ov__acts, .ov__liveacts, .ov__modepick, .ov__langpick, .ov__modemenu, .ov__langmenu, .ov__bar, .ov__bar-tip, .ov__canvas-body, .ov__canvas-tools, .ov__canvas-modemenu, .ov__disp, .ov__chat-body, .ov__chat-input, .ov__chat-tools, .resize-corner-handle",
    )
  ) {
    return;
  }
  const label = windowLabel ?? (() => {
    try {
      return getCurrentWindow().label;
    } catch {
      return "meeting";
    }
  })();
  void getCurrentWindow()
    .startDragging()
    .catch(() => call("meeting_drag", { label }));
}

type MeetingSurface = "host" | "cc" | "canvas" | "chat";

function meetingSurfaceFromLabel(): MeetingSurface {
  try {
    const label = getCurrentWindow().label;
    if (label === "meeting-cc") return "cc";
    if (label === "meeting-canvas") return "canvas";
    if (label === "meeting-chat") return "chat";
  } catch {
    /* fall through */
  }
  return "host";
}

function windowLabelForSurface(surface: MeetingSurface): string {
  if (surface === "cc") return "meeting-cc";
  if (surface === "canvas") return "meeting-canvas";
  if (surface === "chat") return "meeting-chat";
  return "meeting";
}

function defaultPanelSize(surface: MeetingSurface): { w: number; h: number } {
  if (surface === "canvas") return { w: 380, h: 320 };
  if (surface === "chat") return { w: 320, h: 380 };
  if (surface === "cc") return { w: 520, h: 300 };
  return { w: 320, h: 100 };
}

function translationPatchKey(ts: number, speaker: string | null | undefined): string {
  return `${ts}:${speaker ?? ""}`;
}
function useLiveLineBuffer(active: boolean): {
  lines: TranscriptLine[];
  interims: TranscriptLine[];
  pushLine: (line: TranscriptLine) => void;
  upsertInterim: (line: TranscriptLine) => void;
  patchTranslation: (ts: number, speaker: string | null | undefined, translation: string) => void;
  snapshot: () => TranscriptLine[];
} {
  const [lines, setLines] = useState<TranscriptLine[]>([]);
  const [interims, setInterims] = useState<TranscriptLine[]>([]);
  const pendingRef = useRef<TranscriptLine[]>([]);
  const interimRef = useRef<Map<string, TranscriptLine>>(new Map());
  const transRef = useRef<Map<string, string>>(new Map());
  const rafRef = useRef<number | null>(null);

  const flush = useCallback((): void => {
    rafRef.current = null;
    const batch = pendingRef.current;
    const patches = transRef.current;
    const interimBatch = Array.from(interimRef.current.values());
    pendingRef.current = [];
    transRef.current = new Map();
    if (batch.length === 0 && patches.size === 0 && interimBatch.length === 0) {
      // Still refresh interims when map was cleared.
      setInterims(interimBatch);
      return;
    }
    setInterims(interimBatch);
    if (batch.length === 0 && patches.size === 0) return;
    setLines((prev) => {
      let next = batch.length > 0 ? [...prev, ...batch] : prev;
      if (patches.size > 0) {
        next = next.map((l) => {
          const translation = patches.get(translationPatchKey(l.ts, l.speaker));
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
      setInterims([]);
      pendingRef.current = [];
      interimRef.current = new Map();
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
      interimRef.current.delete(line.speaker ?? "");
      pendingRef.current.push(line);
      schedule();
    },
    [schedule],
  );

  const upsertInterim = useCallback(
    (line: TranscriptLine) => {
      interimRef.current.set(line.speaker ?? "", line);
      schedule();
    },
    [schedule],
  );

  const patchTranslation = useCallback(
    (ts: number, speaker: string | null | undefined, translation: string) => {
      const usable = usableTranslation(translation);
      if (!usable) return;
      transRef.current.set(translationPatchKey(ts, speaker), usable);
      schedule();
    },
    [schedule],
  );

  const snapshot = useCallback(
    (): TranscriptLine[] => [...lines, ...pendingRef.current],
    [lines],
  );

  return { lines, interims, pushLine, upsertInterim, patchTranslation, snapshot };
}

const MODES: MeetingMode[] = ["transcription", "one_way", "two_way"];
const LANGS: MeetingLanguage[] = ["auto", "english", "japanese"];
const ONE_WAY_TARGET_LANGS: MeetingLanguage[] = ["english", "japanese"];

export function MeetingOverlay(): JSX.Element | null {
  const surface = meetingSurfaceFromLabel();
  const winLabel = windowLabelForSurface(surface);
  const isHost = surface === "host";
  const [view, setView] = useState<MeetingView | null>(null);
  const [recap, setRecap] = useState<Recap | null>(null);
  const [minutes, setMinutes] = useState<Minutes | null>(null);
  const [transcript, setTranscript] = useState<TranscriptLine[]>([]);
  const [onlyBlanks, setOnlyBlanks] = useState(false);
  const [minutesNeedsKey, setMinutesNeedsKey] = useState(false);
  const [stopping, setStopping] = useState(false);
  /** Optimistic pause toggle — morphs immediately; cleared when backend agrees. */
  const [optimisticPaused, setOptimisticPaused] = useState<boolean | null>(null);
  /** Mirror for event/poll merge — must ignore stale paused:false after clear-on-agree. */
  const optimisticPausedRef = useRef<boolean | null>(null);
  const live = useLiveLineBuffer(view?.state === "recording");
  const liveLines = live.lines;
  const liveInterims = live.interims;
  const [settings, setSettings] = useState<MeetingSettings>({
    meeting_mode: "transcription",
    source_lang: "auto",
    target_lang: "japanese",
    my_lang: "english",
    other_lang: "japanese",
  });
  const [modeOpen, setModeOpen] = useState(false);
  const [langOpen, setLangOpen] = useState<"source" | "target" | "my" | "other" | null>(null);
  const [translateKeyIssue, setTranslateKeyIssue] = useState<"missing" | "invalid" | null>(null);
  const [copyFlash, setCopyFlash] = useState(false);
  /** AI Canvas panel (Notes / document pill) — Live Summary + Timeline. */
  const [notesOpen, setNotesOpen] = useState(false);
  /** Live transcription / captions panel (CC pill). */
  const [ccOn, setCcOn] = useState(true);
  const [chatOn, setChatOn] = useState(false);
  const [dispOpen, setDispOpen] = useState(false);
  const [captionTextSize, setCaptionTextSize] = useState<CaptionTextSize>("s");
  const [captionWeight, setCaptionWeight] = useState<CaptionTextWeight>("light");
  const [showOriginal, setShowOriginal] = useState(true);
  const [captionSplit, setCaptionSplit] = useState<CaptionSplit>("side");
  const [canvasMode, setCanvasMode] = useState<CanvasMode>("live_summary");
  const [canvasModeOpen, setCanvasModeOpen] = useState(false);
  const [canvasSummary, setCanvasSummary] = useState("");
  const [canvasSummaryStatus, setCanvasSummaryStatus] = useState<
    "idle" | "waiting" | "updating" | "needs_key" | "failed"
  >("idle");
  const canvasSummaryFingerprintRef = useRef("");
  const canvasSummaryInFlightRef = useRef(false);
  const [chatMessages, setChatMessages] = useState<ChatMessage[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatBusy, setChatBusy] = useState(false);
  const overlaySizeRef = useRef(defaultPanelSize(surface));
  /** Gate bar :hover until pointer moves — avoids false active look when window spawns under cursor. */
  const [barHoverReady, setBarHoverReady] = useState(false);
  const [audioLevel, setAudioLevel] = useState(0);
  const liveScrollRef = useRef<HTMLDivElement>(null);
  const liveSnapshotRef = useRef<TranscriptLine[]>([]);
  const audioPeakRef = useRef(0);
  const audioHasRealLevelRef = useRef(false);
  const offerProgressRef = useRef<HTMLDivElement>(null);
  const offerDeadlineRef = useRef(0);
  const prevMeetingStateRef = useRef<MeetingView["state"] | null>(null);

  const drag = (e: React.PointerEvent): void => beginMeetingDrag(e, winLabel);

  useEffect(() => {
    liveSnapshotRef.current = live.snapshot();
  }, [liveLines, live.snapshot]);

  /** Apply meeting view; while optimistic pause pending, drop stale paused mismatches. */
  const applyMeetingView = useCallback((incoming: MeetingView) => {
    const opt = optimisticPausedRef.current;
    if (opt !== null && incoming.state === "recording" && incoming.paused !== opt) {
      // In-flight meeting_status / tick raced ahead of toggle — keep optimistic bit.
      setView({ ...incoming, paused: opt });
      return;
    }
    if (opt !== null && incoming.state === "recording" && incoming.paused === opt) {
      optimisticPausedRef.current = null;
      setOptimisticPaused(null);
    }
    setView(incoming);
  }, []);

  useEffect(() => {
    const read = (): void => {
      void invoke<MeetingView>("meeting_status").then(applyMeetingView).catch(() => undefined);
    };
    read();
    const timer = window.setInterval(read, 1000);
    const off = listen<MeetingView>("meeting", (e) => applyMeetingView(e.payload));
    return () => {
      window.clearInterval(timer);
      void off.then((f) => f());
    };
  }, [applyMeetingView]);

  useEffect(() => {
    if (view?.state !== "recording") {
      setTranslateKeyIssue(null);
      return;
    }
    void invoke<MeetingSettings>("get_meeting_settings")
      .then(setSettings)
      .catch(() => undefined);

    void invoke<boolean>("meeting_select_kk_configured")
      .then((configured) => {
        if (!configured) setTranslateKeyIssue("missing");
      })
      .catch(() => undefined);

    const offLine = listen<LiveLineEvent>("meeting_live_line", (e) => {
      const line = e.payload;
      const row: TranscriptLine = {
        ts: line.ts,
        speaker: line.speaker,
        text: line.text,
        translation: line.translation ?? null,
      };
      if (line.interim) {
        live.upsertInterim(row);
      } else {
        live.pushLine(row);
      }
    });
    const offTrans = listen<TranslationEvent>("meeting_live_translation", (e) => {
      const { ts, speaker, translation } = e.payload;
      live.patchTranslation(ts, speaker, translation);
    });
    const offNeedsKey = listen("meeting_translate_needs_key", () => setTranslateKeyIssue("missing"));
    const offKeyInvalid = listen("meeting_translate_key_invalid", () => setTranslateKeyIssue("invalid"));
    return () => {
      void offLine.then((f) => f());
      void offTrans.then((f) => f());
      void offNeedsKey.then((f) => f());
      void offKeyInvalid.then((f) => f());
    };
  }, [view?.state, live.pushLine, live.upsertInterim, live.patchTranslation]);

  useEffect(() => {
    if (view?.state !== "recording") setStopping(false);
  }, [view?.state]);

  useEffect(() => {
    if (view?.state !== "recording") {
      optimisticPausedRef.current = null;
      setOptimisticPaused(null);
    }
  }, [view?.state]);

  const effectivePaused =
    view?.state === "recording" ? (optimisticPaused ?? Boolean(view.paused)) : false;

  useEffect(() => {
    // Only the host drives open flags — panel windows render their surface while Rust shows them.
    if (!isHost || view?.state !== "recording") return;
    call("meeting_set_overlay_panel", { open: ccOn });
  }, [isHost, view?.state, ccOn]);

  useEffect(() => {
    if (!isHost) return;
    const prev = prevMeetingStateRef.current;
    const enteringRecording = view?.state === "recording" && prev !== "recording";
    prevMeetingStateRef.current = view?.state ?? null;

    if (view?.state !== "recording") {
      if (notesOpen) setNotesOpen(false);
      if (chatOn) setChatOn(false);
      setCanvasModeOpen(false);
      setDispOpen(false);
      call("meeting_set_overlay_canvas", { open: false });
      call("meeting_set_overlay_chat", { open: false });
      return;
    }

    if (enteringRecording) {
      if (notesOpen) setNotesOpen(false);
      if (chatOn) setChatOn(false);
    }
    call("meeting_set_overlay_canvas", { open: enteringRecording ? false : notesOpen });
    call("meeting_set_overlay_chat", { open: enteringRecording ? false : chatOn });
  }, [isHost, view?.state, notesOpen, chatOn]);

  useEffect(() => {
    if (!isHost) return;
    const off = listen<{ panel: boolean; canvas: boolean; chat: boolean }>(
      "meeting_overlay_panels",
      (e) => {
        setCcOn(e.payload.panel);
        setNotesOpen(e.payload.canvas);
        setChatOn(e.payload.chat);
      },
    );
    return () => {
      void off.then((f) => f());
    };
  }, [isHost]);

  // Content panels only mount in their own windows; host drives open flags via bar toggles.
  const canvasActive = surface === "canvas";
  const chatActive = surface === "chat";
  const ccActive = surface === "cc";

  useEffect(() => {
    if (!canvasActive || canvasMode !== "live_summary" || view?.state !== "recording") {
      return;
    }
    const turns = groupTurns(liveLines);
    const transcript = turns
      .map((t) => `${t.speakerLabel}: ${t.text.trim()}`)
      .filter((line) => line.length > 2)
      .join("\n");
    if (turns.length < 5 || transcript.length < 420) {
      setCanvasSummaryStatus((s) => (s === "updating" ? s : "waiting"));
      return;
    }
    const fingerprint = `${turns.length}:${transcript.length}:${turns[turns.length - 1]?.ts ?? 0}`;
    if (fingerprint === canvasSummaryFingerprintRef.current) return;
    if (canvasSummaryInFlightRef.current) return;

    const timer = window.setTimeout(() => {
      if (canvasSummaryInFlightRef.current) return;
      canvasSummaryInFlightRef.current = true;
      setCanvasSummaryStatus("updating");
      void invoke("meeting_request_live_summary", { transcript })
        .then(() => {
          canvasSummaryFingerprintRef.current = fingerprint;
        })
        .catch((err: unknown) => {
          canvasSummaryInFlightRef.current = false;
          const msg = String(err);
          if (msg.includes("need_more_context") || msg.includes("rate_limited") || msg.includes("in_flight")) {
            setCanvasSummaryStatus((s) => (canvasSummary ? s : "waiting"));
          } else if (msg.includes("needs_key")) {
            setCanvasSummaryStatus("needs_key");
          } else {
            setCanvasSummaryStatus("failed");
          }
        });
    }, 22_000);
    return () => window.clearTimeout(timer);
  }, [canvasActive, canvasMode, liveLines, view?.state, canvasSummary]);

  useEffect(() => {
    const offOk = listen<{ summary: string }>("meeting_live_summary", (e) => {
      canvasSummaryInFlightRef.current = false;
      const text = e.payload.summary?.trim() ?? "";
      if (text) {
        setCanvasSummary(text);
        setCanvasSummaryStatus("idle");
      }
    });
    const offKey = listen("meeting_live_summary_needs_key", () => {
      canvasSummaryInFlightRef.current = false;
      setCanvasSummaryStatus("needs_key");
    });
    const offFail = listen("meeting_live_summary_failed", () => {
      canvasSummaryInFlightRef.current = false;
      setCanvasSummaryStatus("failed");
    });
    return () => {
      void offOk.then((f) => f());
      void offKey.then((f) => f());
      void offFail.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (view?.state === "recording") return;
    setCanvasSummary("");
    setCanvasSummaryStatus("idle");
    canvasSummaryFingerprintRef.current = "";
    canvasSummaryInFlightRef.current = false;
  }, [view?.state]);

  useEffect(() => {
    if (!ccOn) {
      setDispOpen(false);
      return;
    }
    setModeOpen(false);
    setLangOpen(null);
  }, [ccOn]);

  useEffect(() => {
    if (view?.state !== "recording") {
      setBarHoverReady(false);
      return;
    }
    const enable = (): void => setBarHoverReady(true);
    window.addEventListener("pointermove", enable, { once: true });
    return () => window.removeEventListener("pointermove", enable);
  }, [view?.state]);

  useEffect(() => {
    if (view?.state !== "recording" || effectivePaused) {
      audioHasRealLevelRef.current = false;
      setAudioLevel(0);
      return;
    }
    const off = listen<{ rms: number }>("meeting_level", (e) => {
      audioHasRealLevelRef.current = true;
      const rms = e.payload.rms;
      audioPeakRef.current = Math.max(audioPeakRef.current * 0.85, rms);
      const norm = audioPeakRef.current > 0 ? Math.min(1, rms / audioPeakRef.current) : 0;
      setAudioLevel(norm);
    });
    let raf = 0;
    let t0 = performance.now();
    const pulse = (now: number): void => {
      if (!audioHasRealLevelRef.current) {
        const phase = (now - t0) / 1000;
        const wave = 0.22 + 0.55 * (0.5 + 0.5 * Math.sin(phase * 3.1));
        setAudioLevel(wave);
      }
      raf = window.requestAnimationFrame(pulse);
    };
    raf = window.requestAnimationFrame(pulse);
    return () => {
      window.cancelAnimationFrame(raf);
      void off.then((f) => f());
    };
  }, [view?.state, effectivePaused]);

  useEffect(() => {
    if (view?.state !== "offered") return;
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      call("meeting_not_now");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view?.state]);

  // Backend tick is 1 Hz; sync deadline on each payload, animate width on rAF (no React churn).
  useEffect(() => {
    if (view?.state !== "offered") {
      offerDeadlineRef.current = 0;
      return;
    }
    offerDeadlineRef.current = Date.now() + view.countdown_ms;
  }, [view?.state, view?.countdown_ms]);

  useEffect(() => {
    if (view?.state !== "offered") return;
    let raf = 0;
    const step = (): void => {
      const remain = Math.max(0, offerDeadlineRef.current - Date.now());
      const pct = Math.max(0, Math.min(1, remain / OFFER_GRACE_MS));
      const el = offerProgressRef.current;
      if (el) {
        el.style.width = `${pct * 100}%`;
        el.setAttribute("aria-valuenow", String(Math.round(pct * 100)));
      }
      if (remain > 0) raf = window.requestAnimationFrame(step);
    };
    raf = window.requestAnimationFrame(step);
    return () => window.cancelAnimationFrame(raf);
  }, [view?.state]);

  useEffect(() => {
    liveScrollRef.current?.scrollTo({
      top: liveScrollRef.current.scrollHeight,
      behavior: "smooth",
    });
  }, [liveLines.length, liveInterims.length]);

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

  if (!view || !view.enabled || view.state === "idle") return null;
  // Panel windows only paint while recording; offer/recap stay on the host.
  if (!isHost && view.state !== "recording") return null;

  const name = view.title?.trim() || t.meetingUntitled;

  const handleStop = (): void => {
    if (stopping) return;
    setStopping(true);
    call("meeting_stop");
  };

  const handleTogglePause = (): void => {
    if (view?.state !== "recording") return;
    const next = !effectivePaused;
    optimisticPausedRef.current = next;
    // Paint morph this frame — do not wait for invoke / backend emit.
    flushSync(() => setOptimisticPaused(next));
    void invoke("meeting_toggle_pause").catch(() => {
      optimisticPausedRef.current = null;
      setOptimisticPaused(null);
    });
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
    const remainPct = Math.round(
      Math.max(0, Math.min(100, (view.countdown_ms / OFFER_GRACE_MS) * 100)),
    );
    return (
      <div
        className="ov-offer"
        onPointerDown={drag}
        title={t.meetingDisclosureBrief}
      >
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
              <button
                type="button"
                className="ov__offer-go"
                onClick={() => call("meeting_start")}
              >
                <img
                  className="ov__offer-ico"
                  src={meetingIcon}
                  alt=""
                  width={18}
                  height={18}
                  draggable={false}
                />
                {t.meetingTakeNotes}
              </button>
              <button
                type="button"
                className="ov__offer-skip"
                onClick={() => call("meeting_not_now")}
              >
                {t.meetingNotNow}
              </button>
            </div>
          </div>
          <div
            ref={offerProgressRef}
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

  if (view.state === "recording") {
    const translating = settings.meeting_mode !== "transcription";
    const liveTurns = groupTurns([...liveLines, ...liveInterims]);

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

    const toggleNotes = (): void => {
      setNotesOpen((open) => !open);
      setCanvasModeOpen(false);
      setModeOpen(false);
      setLangOpen(null);
    };

    const timelineSteps = buildTimeline(liveLines);
    const onPanelResize = (w: number, h: number): void => {
      overlaySizeRef.current = { w, h };
      call("meeting_set_overlay_size", { width: w, height: h, label: winLabel });
    };

    const closeCanvas = (): void => {
      setCanvasModeOpen(false);
      if (isHost) {
        setNotesOpen(false);
      } else {
        call("meeting_set_overlay_canvas", { open: false });
      }
    };

    const closeCc = (): void => {
      setModeOpen(false);
      setLangOpen(null);
      setDispOpen(false);
      if (isHost) {
        setCcOn(false);
      } else {
        call("meeting_set_overlay_panel", { open: false });
      }
    };

    const closeChat = (): void => {
      if (isHost) {
        setChatOn(false);
      } else {
        call("meeting_set_overlay_chat", { open: false });
      }
    };

    const canvasPanel = canvasActive ? (
      <div className="ov__canvas">
        <header className="ov__canvas-head">
          <div className="ov__canvas-titles ov__drag" onPointerDown={drag}>
            <div className="ov__canvas-title">{t.meetingAiCanvas}</div>
            <div className="ov__canvas-status">
              {effectivePaused ? t.meetingCanvasPaused : t.meetingCanvasListening}
            </div>
          </div>
          <DragHandle6Dot
            className="ov__drag ov__panel-grip"
            title={t.meetingCanvasDrag}
            onPointerDown={drag}
          />
          <div className="ov__canvas-tools ov__nodrag" data-no-drag>
            <div className="ov__canvas-modepick">
              <button
                type="button"
                className="ov__canvas-modebtn"
                aria-expanded={canvasModeOpen}
                onClick={() => setCanvasModeOpen((o) => !o)}
              >
                {canvasMode === "timeline" ? t.meetingCanvasTimeline : t.meetingCanvasLiveSummary}
                <span className="ov__chev" aria-hidden />
              </button>
              {canvasModeOpen ? (
                <div className="ov__canvas-modemenu" role="listbox">
                  <button
                    type="button"
                    role="option"
                    aria-selected={canvasMode === "live_summary"}
                    className={`ov__canvas-modeopt${canvasMode === "live_summary" ? " is-on" : ""}`}
                    onClick={() => {
                      setCanvasMode("live_summary");
                      setCanvasModeOpen(false);
                    }}
                  >
                    <span className="ov__canvas-modeopt-main">
                      {canvasMode === "live_summary" ? (
                        <span className="ov__canvas-check" aria-hidden>
                          ✓
                        </span>
                      ) : (
                        <span className="ov__canvas-check-spacer" aria-hidden />
                      )}
                      {t.meetingCanvasLiveSummary}
                    </span>
                    <span className="ov__canvas-tag">{t.meetingCanvasOfficial}</span>
                  </button>
                  <button
                    type="button"
                    role="option"
                    aria-selected={canvasMode === "timeline"}
                    className={`ov__canvas-modeopt${canvasMode === "timeline" ? " is-on" : ""}`}
                    onClick={() => {
                      setCanvasMode("timeline");
                      setCanvasModeOpen(false);
                    }}
                  >
                    <span className="ov__canvas-modeopt-main">
                      {canvasMode === "timeline" ? (
                        <span className="ov__canvas-check" aria-hidden>
                          ✓
                        </span>
                      ) : (
                        <span className="ov__canvas-check-spacer" aria-hidden />
                      )}
                      {t.meetingCanvasTimeline}
                    </span>
                    <span className="ov__canvas-tag">{t.meetingCanvasOfficial}</span>
                  </button>
                  <div className="ov__canvas-menudiv" role="separator" />
                  <button type="button" className="ov__canvas-modeopt is-disabled" disabled>
                    <span className="ov__canvas-modeopt-main">
                      <span className="ov__canvas-check-spacer" aria-hidden />
                      {t.meetingCanvasManage}
                    </span>
                    <span className="ov__canvas-ext" aria-hidden>
                      ↗
                    </span>
                  </button>
                </div>
              ) : null}
            </div>
            <button
              type="button"
              className="ov__iconbtn"
              title={t.meetingCloseNote}
              aria-label={t.meetingCloseNote}
              onClick={closeCanvas}
            >
              <img className="ov__icon" src={closeIcon} alt="" width={18} height={18} draggable={false} />
            </button>
          </div>
        </header>

        <div className="ov__canvas-body ov__nodrag" data-no-drag>
          {canvasMode === "live_summary" ? (
            canvasSummary ? (
              <>
                <p className="ov__canvas-summary">{canvasSummary}</p>
                {canvasSummaryStatus === "updating" ? (
                  <p className="ov__canvas-empty ov__canvas-empty--soft">{t.meetingCanvasSummaryUpdating}</p>
                ) : null}
              </>
            ) : (
              <p className="ov__canvas-empty">
                {canvasSummaryStatus === "needs_key"
                  ? t.meetingCanvasSummaryNeedsKey
                  : canvasSummaryStatus === "failed"
                    ? t.meetingCanvasSummaryFailed
                    : canvasSummaryStatus === "updating"
                      ? t.meetingCanvasSummaryUpdating
                      : t.meetingCanvasSummaryWaiting}
              </p>
            )
          ) : timelineSteps.length > 0 ? (
            <ol className="ov__canvas-timeline">
              {timelineSteps.map((step, i) => (
                <li className="ov__canvas-step" key={`${step.ts}-${i}`}>
                  <div className="ov__canvas-step-rail" aria-hidden>
                    <span className="ov__canvas-step-dot" />
                    {i < timelineSteps.length - 1 ? <span className="ov__canvas-step-line" /> : null}
                  </div>
                  <div className="ov__canvas-step-body">
                    <div className="ov__canvas-step-meta">
                      <span className="ov__canvas-step-label">{step.label}</span>
                      <span className="ov__canvas-step-time">{clock(step.ts)}</span>
                    </div>
                    <p className="ov__canvas-step-detail">{step.detail}</p>
                  </div>
                </li>
              ))}
            </ol>
          ) : (
            <p className="ov__canvas-empty">{t.meetingCanvasTimelineEmpty}</p>
          )}
        </div>

        <ResizeCornerHandle
          getSize={() => overlaySizeRef.current}
          onResize={onPanelResize}
          anchor="top-left"
          min={{ w: 320, h: 220 }}
          max={{ w: 720, h: 900 }}
          title={t.resizeHint}
          className="ov__canvas-resize"
        />
      </div>
    ) : null;

    const toggleCc = (): void => {
      setCcOn((open) => !open);
      setModeOpen(false);
      setLangOpen(null);
      setDispOpen(false);
    };

    const toggleChat = (): void => {
      setChatOn((open) => !open);
      setModeOpen(false);
      setDispOpen(false);
    };

    const sendChat = (): void => {
      const q = chatInput.trim();
      if (!q || chatBusy) return;
      setChatInput("");
      setChatMessages((prev) => [...prev, { role: "user", text: q }]);
      setChatBusy(true);
      const recent = liveLines
        .slice(-30)
        .map((l) => `${speakerLabel(l.speaker)}: ${l.text}`)
        .join("\n");
      // Prefixed context for BYOK lane — never logged from the webview.
      const message = recent
        ? `You are answering about the current meeting. Use only this transcript context:\n---\n${recent}\n---\nQuestion: ${q}`
        : q;
      void invoke<{ text: string }>("shogun_chat", { message })
        .then((ans) => {
          const text = ans.text?.trim() || t.noAnswer;
          setChatMessages((prev) => [...prev, { role: "assistant", text }]);
        })
        .catch(() => {
          setChatMessages((prev) => [...prev, { role: "assistant", text: t.meetingChatStub }]);
        })
        .finally(() => setChatBusy(false));
    };

    const seg = <T extends string>(
      options: { value: T; label: string }[],
      value: T,
      onChange: (v: T) => void,
    ): JSX.Element => (
      <div className="ov__seg" role="group">
        {options.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={`ov__seg-btn${opt.value === value ? " is-on" : ""}`}
            aria-pressed={opt.value === value}
            onClick={() => onChange(opt.value)}
          >
            {opt.label}
          </button>
        ))}
      </div>
    );

    const displaySettings =
      dispOpen && ccActive ? (
        <div className="ov__disp" role="dialog" aria-label={t.meetingDisplaySettings} data-no-drag>
          <div className="ov__disp-title">{t.meetingDisplaySettings}</div>
          {settings.meeting_mode === "one_way" ? (
            <div className="ov__disp-row">
              <span className="ov__disp-label">
                <span className="ov__disp-ico" aria-hidden>
                  文A
                </span>
                {t.meetingDisplayOriginal}
              </span>
              <button
                type="button"
                className={`ov__toggle${showOriginal ? " is-on" : ""}`}
                role="switch"
                aria-checked={showOriginal}
                onClick={() => setShowOriginal((v) => !v)}
              >
                <span className="ov__toggle-knob" />
              </button>
            </div>
          ) : null}
          <div className="ov__disp-row">
            <span className="ov__disp-label">
              <span className="ov__disp-ico" aria-hidden>
                Aa
              </span>
              {t.meetingDisplayText}
            </span>
            {seg(
              [
                { value: "s" as const, label: t.meetingDisplaySizeS },
                { value: "m" as const, label: t.meetingDisplaySizeM },
                { value: "l" as const, label: t.meetingDisplaySizeL },
              ],
              captionTextSize,
              setCaptionTextSize,
            )}
          </div>
          <div className="ov__disp-row">
            <span className="ov__disp-label">
              <span className="ov__disp-ico" aria-hidden>
                B
              </span>
              {t.meetingDisplayWeight}
            </span>
            {seg(
              [
                { value: "light" as const, label: t.meetingDisplayWeightLight },
                { value: "bold" as const, label: t.meetingDisplayWeightBold },
              ],
              captionWeight,
              setCaptionWeight,
            )}
          </div>
          {settings.meeting_mode !== "transcription" ? (
            <div className="ov__disp-row">
              <span className="ov__disp-label">
                <span className="ov__disp-ico" aria-hidden>
                  ▤
                </span>
                {t.meetingDisplaySplit}
              </span>
              {seg(
                [
                  { value: "side" as const, label: t.meetingDisplaySplitSide },
                  { value: "stack" as const, label: t.meetingDisplaySplitStack },
                ],
                captionSplit,
                setCaptionSplit,
              )}
            </div>
          ) : null}
        </div>
      ) : null;

    const chatPanel = chatActive ? (
      <div className="ov__chat">
        <header className="ov__chat-head">
          <button
            type="button"
            className="ov__iconbtn"
            title={t.meetingChatNew}
            aria-label={t.meetingChatNew}
            data-no-drag
            onClick={() => setChatMessages([])}
          >
            <img className="ov__icon" src={notesIcon} alt="" width={16} height={16} draggable={false} />
          </button>
          <DragHandle6Dot
            className="ov__drag ov__panel-grip"
            title={t.meetingCanvasDrag}
            onPointerDown={drag}
          />
          <button
            type="button"
            className="ov__iconbtn"
            title={t.meetingChatClose}
            aria-label={t.meetingChatClose}
            data-no-drag
            onClick={closeChat}
          >
            <img className="ov__icon" src={closeIcon} alt="" width={18} height={18} draggable={false} />
          </button>
        </header>
        <div className="ov__chat-body ov__nodrag" data-no-drag>
          {chatMessages.length === 0 ? (
            <p className="ov__chat-empty">{t.meetingChatEmpty}</p>
          ) : (
            chatMessages.map((m, i) => (
              <div key={i} className={`ov__chat-msg ov__chat-msg--${m.role}`}>
                {m.text}
              </div>
            ))
          )}
        </div>
        <div className="ov__chat-inputwrap ov__nodrag" data-no-drag>
          <input
            className="ov__chat-input"
            type="text"
            value={chatInput}
            placeholder={t.meetingChatPlaceholder}
            disabled={chatBusy}
            onChange={(e) => setChatInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                sendChat();
              }
            }}
          />
          <div className="ov__chat-trail">
            <button
              type="button"
              className="ov__chat-send"
              title={t.meetingChatSend}
              aria-label={t.meetingChatSend}
              disabled={chatBusy || !chatInput.trim()}
              onClick={sendChat}
            >
              ↑
            </button>
          </div>
        </div>
        <ResizeCornerHandle
          getSize={() => overlaySizeRef.current}
          onResize={onPanelResize}
          anchor="top-left"
          min={{ w: 300, h: 360 }}
          max={{ w: 520, h: 900 }}
          title={t.resizeHint}
          className="ov__chat-resize"
        />
      </div>
    ) : null;

    const paused = effectivePaused;
    const waveHeights = [0.35, 0.85, 0.55, 0.95, 0.45].map((base, i) => {
      const wobble = 0.15 * Math.sin(audioLevel * 12 + i * 1.7);
      return Math.max(0.18, Math.min(1, base * (0.35 + audioLevel * 0.9) + wobble));
    });

    const controlBar = (
      <div className={`ov__bar ov__nodrag${barHoverReady ? " ov__bar--hover-ready" : ""}`} data-no-drag>
        <div className="ov__bar-cluster" role="toolbar" aria-label={t.meetingNotes}>
          <div className="ov__bar-slot">
            <button
              type="button"
              className={`ov__bar-btn${notesOpen ? " is-on" : ""}`}
              aria-pressed={notesOpen}
              aria-label={notesOpen ? t.meetingCloseNote : t.meetingOpenNotes}
              onClick={toggleNotes}
            >
              <img className="ov__bar-ico" src={notesIcon} alt="" width={20} height={20} draggable={false} />
            </button>
            <span className="ov__bar-tip" role="tooltip">
              {notesOpen ? t.meetingCloseNote : t.meetingOpenNotes}
            </span>
          </div>
          <div className="ov__bar-slot">
            <button
              type="button"
              className={`ov__bar-btn${ccOn ? " is-on" : ""}`}
              aria-pressed={ccOn}
              aria-label={ccOn ? t.meetingCloseCaptions : t.meetingOpenCaptions}
              onClick={toggleCc}
            >
              <img className="ov__bar-ico" src={ccIcon} alt="" width={20} height={20} draggable={false} />
            </button>
            <span className="ov__bar-tip" role="tooltip">
              {ccOn ? t.meetingCloseCaptions : t.meetingOpenCaptions}
            </span>
          </div>
          <div className="ov__bar-slot">
            <button
              type="button"
              className={`ov__bar-btn${chatOn ? " is-on" : ""}`}
              aria-pressed={chatOn}
              aria-label={chatOn ? t.meetingCloseChat : t.meetingOpenChat}
              onClick={toggleChat}
            >
              <img className="ov__bar-ico" src={chatIcon} alt="" width={20} height={20} draggable={false} />
            </button>
            <span className="ov__bar-tip" role="tooltip">
              {chatOn ? t.meetingCloseChat : t.meetingOpenChat}
            </span>
          </div>
          <div className="ov__bar-slot ov__bar-slot--more">
            <button
              type="button"
              className={`ov__bar-btn${modeOpen && !ccOn ? " is-on" : ""}`}
              aria-expanded={modeOpen && !ccOn}
              aria-label={t.meetingMore}
              onClick={() => {
                if (ccOn) return;
                setModeOpen(!modeOpen);
                setLangOpen(null);
              }}
            >
              <img className="ov__bar-ico" src={moreIcon} alt="" width={20} height={20} draggable={false} />
            </button>
            <span className="ov__bar-tip" role="tooltip">
              {t.meetingMore}
            </span>
            {modeOpen && !ccOn ? (
              <div className="ov__modemenu ov__modemenu--bar" role="listbox">
                {MODES.map((m) => (
                  <button
                    key={m}
                    type="button"
                    role="option"
                    aria-selected={m === settings.meeting_mode}
                    className={`ov__modeopt${m === settings.meeting_mode ? " is-on" : ""}`}
                    onClick={() => setMode(m)}
                  >
                    {m === settings.meeting_mode ? (
                      <span className="ov__mode-check" aria-hidden>
                        ✓
                      </span>
                    ) : (
                      <span className="ov__mode-check-spacer" aria-hidden />
                    )}
                    {modeLabel(m)}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        <span className="ov__bar-div" aria-hidden />

        <button
          type="button"
          className={`ov__wave${paused ? " is-paused" : " is-live"}`}
          aria-pressed={paused}
          aria-label={paused ? t.meetingResume : t.meetingPause}
          title={paused ? t.meetingResume : t.meetingPause}
          onClick={handleTogglePause}
        >
          {/* Both glyphs stay mounted so pause↔resume morphs (no hard cut). */}
          <span className="ov__wave-glyph ov__wave-glyph--bars" aria-hidden>
            {waveHeights.map((h, i) => (
              <span
                key={i}
                className="ov__wave-bar"
                style={{ ["--h" as string]: `${Math.round(h * 100)}%` }}
              />
            ))}
          </span>
          <span className="ov__wave-glyph ov__wave-glyph--play" aria-hidden>
            <svg
              className="ov__wave-play"
              width={21}
              height={21}
              viewBox="0 0 24 24"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
              aria-hidden
            >
              <path
                d="M8.9 6.5c0-.82.9-1.32 1.62-.88l8.2 5.15c.7.44.7 1.52 0 1.96l-8.2 5.15c-.72.45-1.62-.06-1.62-.88V6.5Z"
                fill="currentColor"
                stroke="currentColor"
                strokeWidth="1.15"
                strokeLinejoin="round"
                strokeLinecap="round"
              />
            </svg>
          </span>
        </button>

        <button
          type="button"
          className="ov__bar-stop"
          disabled={stopping}
          aria-label={t.meetingEndMeeting}
          title={t.meetingEndMeeting}
          onClick={handleStop}
        >
          <img className="ov__bar-stop-ico" src={stopIcon} alt="" width={36} height={36} draggable={false} />
        </button>
      </div>
    );

    const captionTone = `ov__cap--${captionTextSize} ov__cap--${captionWeight}`;

    const captionsPanel = ccActive ? (
      <div className={`ov__live ${captionTone}`}>
        <header className="ov__livehead">
          <div className="ov__livehead-left ov__nodrag" data-no-drag>
            <div className="ov__modepick">
              <button
                type="button"
                className="ov__modebtn"
                aria-expanded={modeOpen}
                onClick={() => {
                  setModeOpen(!modeOpen);
                  setLangOpen(null);
                  setDispOpen(false);
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
                      {m === settings.meeting_mode ? (
                        <span className="ov__mode-check" aria-hidden>
                          ✓
                        </span>
                      ) : (
                        <span className="ov__mode-check-spacer" aria-hidden />
                      )}
                      {modeLabel(m)}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
            {!translating ? (
              <button
                type="button"
                className="ov__iconbtn"
                title={copyFlash ? t.meetingCopiedTranscript : t.meetingCopyTranscript}
                aria-label={t.meetingCopyTranscript}
                onClick={() => {
                  const text = liveTurns.map((turn) => turn.text).join("\n");
                  if (!text.trim()) return;
                  void navigator.clipboard.writeText(text).then(() => {
                    setCopyFlash(true);
                    window.setTimeout(() => setCopyFlash(false), 1200);
                  });
                }}
              >
                <IconCopy className="ov__icon" />
              </button>
            ) : null}
          </div>

          <DragHandle6Dot
            className="ov__drag ov__panel-grip ov__live-grip"
            title={t.meetingCanvasDrag}
            onPointerDown={drag}
          />

          <div className="ov__liveacts ov__nodrag" data-no-drag>
            {settings.meeting_mode === "one_way" ? (
              <div className="ov__langrow ov__langrow--header">
                {langPicker("source", "source_lang", settings.source_lang, LANGS)}
                <span className="ov__langarrow">{t.meetingLangArrow}</span>
                {langPicker("target", "target_lang", settings.target_lang, ONE_WAY_TARGET_LANGS)}
              </div>
            ) : null}
            {settings.meeting_mode === "two_way" ? (
              <div className="ov__langrow ov__langrow--header">
                {langPicker("my", "my_lang", settings.my_lang, ONE_WAY_TARGET_LANGS)}
                <span className="ov__langarrow">{t.meetingLangSwap}</span>
                {langPicker("other", "other_lang", settings.other_lang, ONE_WAY_TARGET_LANGS)}
              </div>
            ) : null}
            <div className="ov__disp-anchor">
              <button
                type="button"
                className={`ov__iconbtn${dispOpen ? " is-on" : ""}`}
                title={t.meetingCaptionsSettings}
                aria-label={t.meetingCaptionsSettings}
                aria-expanded={dispOpen}
                onClick={() => {
                  setDispOpen((o) => !o);
                  setModeOpen(false);
                  setLangOpen(null);
                }}
              >
                <img
                  className="ov__icon"
                  src={preferenceIcon}
                  alt=""
                  width={18}
                  height={18}
                  draggable={false}
                />
              </button>
              {displaySettings}
            </div>
            <button
              type="button"
              className="ov__iconbtn"
              title={t.meetingCloseCaptionsPanel}
              aria-label={t.meetingCloseCaptionsPanel}
              onClick={closeCc}
            >
              <img className="ov__icon" src={closeIcon} alt="" width={18} height={18} draggable={false} />
            </button>
          </div>
        </header>

        <div
          className={`ov__livebody ov__nodrag${translating ? " ov__livebody--split" : ""}`}
          data-no-drag
          ref={liveScrollRef}
        >
          {translateKeyIssue && translating ? (
            <p className="ov__mdegraded ov__mdegraded--warn">
              {translateKeyIssue === "invalid"
                ? t.meetingTranslateKeyInvalid
                : t.meetingTranslateNeedsKey}
            </p>
          ) : null}
          {liveTurns.length === 0 ? (
            <p className="ov__liveempty">{t.meetingLiveEmpty}</p>
          ) : translating && captionSplit === "stack" ? (
            <div
              className={`ov__stack${
                settings.meeting_mode === "one_way" && !showOriginal ? " ov__stack--dst-only" : ""
              }`}
            >
              {liveTurns.map((turn, i) => {
                const translation = usableTranslation(turn.translation);
                const hideSrc = settings.meeting_mode === "one_way" && !showOriginal;
                return (
                  <div className="ov__stack-turn" key={`stack-${turn.ts}-${i}`}>
                    {!hideSrc ? <p className="ov__stack-src">{turn.text}</p> : null}
                    <p
                      className={`ov__stack-dst${translation ? "" : " is-pending"}`}
                      aria-busy={!translation}
                    >
                      {translation ?? ""}
                    </p>
                  </div>
                );
              })}
            </div>
          ) : translating ? (
            <div
              className={`ov__split${
                settings.meeting_mode === "one_way" && !showOriginal ? " ov__split--dst-only" : ""
              }`}
            >
              {!(settings.meeting_mode === "one_way" && !showOriginal) ? (
                <div className="ov__split-col ov__split-col--src">
                  {liveTurns.map((turn, i) => (
                    <p className="ov__split-line" key={`src-${turn.ts}-${i}`}>
                      {turn.text}
                    </p>
                  ))}
                </div>
              ) : null}
              {!(settings.meeting_mode === "one_way" && !showOriginal) ? (
                <div className="ov__split-div" aria-hidden />
              ) : null}
              <div className="ov__split-col ov__split-col--dst">
                {liveTurns.map((turn, i) => {
                  const translation = usableTranslation(turn.translation);
                  return (
                    <p
                      className={`ov__split-line${translation ? "" : " is-pending"}`}
                      key={`dst-${turn.ts}-${i}`}
                      aria-busy={!translation}
                    >
                      {translation ?? ""}
                    </p>
                  );
                })}
              </div>
            </div>
          ) : (
            <div className="ov__transcript">
              {liveTurns.map((turn, i) => (
                <p className="ov__transcript-line" key={`${turn.ts}-${i}`}>
                  {turn.text}
                </p>
              ))}
            </div>
          )}
        </div>

        <ResizeCornerHandle
          getSize={() => overlaySizeRef.current}
          onResize={onPanelResize}
          anchor="top-left"
          min={{ w: 320, h: 220 }}
          max={{ w: 720, h: 900 }}
          title={t.resizeHint}
          className="ov__live-resize"
        />
      </div>
    ) : null;

    // Host = control bar only. Each content surface lives in its own glass window.
    if (surface === "cc") {
      return <div className="ov-panel-shell ov--panel-solo">{captionsPanel}</div>;
    }
    if (surface === "canvas") {
      return <div className="ov-panel-shell ov--panel-solo">{canvasPanel}</div>;
    }
    if (surface === "chat") {
      return <div className="ov-panel-shell ov--panel-solo">{chatPanel}</div>;
    }
    return (
      <div className="ov ov--live ov--live-pill" onPointerDown={drag}>
        {controlBar}
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
    <div className="ov ov--recap" onPointerDown={drag}>
      <div className="ov__recap">
        <header className="ov__rhead">
          <div className="ov__rhead-main ov__drag" onPointerDown={drag}>
            <div className="ov__rkicker">{t.meetingRecapTitle}</div>
            <div className="ov__rtitle">{recap?.title ?? name}</div>
          </div>
          <div className="ov__rhead-tools">
            {recap?.duration_minutes != null ? (
              <span className="ov__rmin">
                {recap.duration_minutes} {t.meetingRecapMinutes}
              </span>
            ) : null}
            <DragHandle6Dot
              className="ov__drag ov__panel-grip"
              title={t.meetingNotes}
              onPointerDown={drag}
            />
          </div>
        </header>

        <div className="ov__rbody ov__nodrag">
          {showWrapping ? (
            <div className="ov__mstatus">
              <span className="ov__mstatus-dot" aria-hidden />
              <p className="ov__mpending">{t.meetingMinutesPending}</p>
            </div>
          ) : null}

          {minutesReady ? (
            <div className="ov__minutes ov__minutes--lead">
              {minutes?.summary ? (
                <section className="ov__msec ov__msec--summary">
                  <div className="ov__mhead">{t.meetingMinutesSummary}</div>
                  <p className="ov__msummary">{minutes.summary}</p>
                </section>
              ) : null}
              {minutes && minutes.decisions.length > 0 ? (
                <section className="ov__msec">
                  <div className="ov__mhead">{t.meetingMinutesDecisions}</div>
                  <ul className="ov__mlist">
                    {minutes.decisions.map((d, i) => (
                      <li key={i}>{d}</li>
                    ))}
                  </ul>
                </section>
              ) : null}
              {minutes && minutes.next_actions.length > 0 ? (
                <section className="ov__msec">
                  <div className="ov__mhead">{t.meetingMinutesNextActions}</div>
                  <ul className="ov__mlist ov__mlist--actions">
                    {minutes.next_actions.map((a, i) => (
                      <li key={i}>
                        <span className="ov__maction">{a.text}</span>
                        {a.owner ? <span className="ov__mowner">{a.owner}</span> : null}
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}
            </div>
          ) : null}

          {showMinutesPending ? (
            <div className="ov__mstatus">
              <span className="ov__mstatus-dot" aria-hidden />
              <p className="ov__mpending">{t.meetingMinutesPending}</p>
            </div>
          ) : null}
          {showMinutesNeedsKey ? (
            <p className="ov__mdegraded">{t.meetingMinutesNeedsKey}</p>
          ) : null}

          {recap?.notes ? (
            <section className="ov__notes">
              <div className="ov__mhead">{t.meetingRecapYourNotes}</div>
              <pre className="ov__rnotes">{recap.notes}</pre>
            </section>
          ) : !minutesReady && !hasTranscript ? (
            <div className="ov__rempty">{t.meetingRecapNoNotes}</div>
          ) : null}

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
            ) : onlyBlanks ? (
              <p className="ov__tempty">{t.meetingTranscriptOnlyBlanks}</p>
            ) : (
              <p className="ov__tempty">{t.meetingTranscriptEmpty}</p>
            )}
          </section>
        </div>

        <footer className="ov__rfoot ov__nodrag">
          <p className="ov__disclosure">{t.meetingDisclosureRecap}</p>
          <button
            type="button"
            className="ov__rdone"
            onClick={() => call("meeting_wrapped")}
          >
            {t.meetingRecapDone}
          </button>
        </footer>
      </div>
    </div>
  );
}
