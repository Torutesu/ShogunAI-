import type { MeetingLanguage, MeetingMode, TranscriptLine } from "./transforms";

export interface MeetingView {
  state: "idle" | "offered" | "recording" | "wrapping";
  enabled: boolean;
  title: string | null;
  app_bundle_id: string | null;
  elapsed_ms: number;
  countdown_ms: number;
  /** Capture/ASR paused; meeting session still open (waveform toggle). */
  paused: boolean;
  /** Capture did not start; typed notes still remain usable. */
  audio_error: string | null;
}

export interface Recap {
  title: string;
  duration_minutes: number | null;
  notes: string | null;
  captured_events: number;
  degraded: boolean;
}

export interface MeetingTranscript {
  lines: TranscriptLine[];
  only_blanks: boolean;
}

export interface MeetingSettings {
  meeting_mode: MeetingMode;
  source_lang: MeetingLanguage;
  target_lang: MeetingLanguage;
  my_lang: MeetingLanguage;
  other_lang: MeetingLanguage;
}

export interface LiveLineEvent {
  ts: number;
  speaker: string | null;
  text: string;
  translation?: string | null;
  interim?: boolean;
}

export interface TranslationEvent {
  ts: number;
  speaker?: string | null;
  translation: string;
}

export type CanvasMode = "live_summary" | "timeline";
export type CaptionTextSize = "s" | "m" | "l";
export type CaptionTextWeight = "light" | "bold";
export type CaptionSplit = "side" | "stack";

export interface ChatMessage {
  role: "user" | "assistant";
  text: string;
}
