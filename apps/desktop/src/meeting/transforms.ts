// Pure transforms behind the meeting overlay (Issue #7 / #122): transcript grouping, the
// timeline, translation hygiene, and small label helpers. No Tauri, no React, no timers — this
// module exists so the logic the overlay leans on hardest is unit-testable without a webview
// (#119), and so the component can memoize against stable functions instead of re-deriving
// closures every render.

import { t } from "../strings";

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

export type MeetingMode = "transcription" | "one_way" | "two_way";
export type MeetingLanguage = "english" | "japanese" | "auto";

export interface TranscriptTurn {
  speakerLabel: string;
  ts: number;
  text: string;
  translation?: string | null;
}

export interface TimelineStep {
  ts: number;
  label: string;
  detail: string;
}

/** mm:ss. Tabular figures in CSS keep the row from reflowing as the seconds tick. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

export function speakerLabel(speaker: string | null): string {
  if (speaker === "me") return t.meetingTranscriptSpeakerMe;
  if (speaker === "other") return t.meetingTranscriptSpeakerOther;
  return t.meetingTranscriptSpeakerUnknown;
}

export function langLabel(lang: MeetingLanguage): string {
  if (lang === "auto") return t.meetingLangAuto;
  if (lang === "japanese") return t.meetingLangJapanese;
  return t.meetingLangEnglish;
}

export function modeLabel(mode: MeetingMode): string {
  if (mode === "one_way") return t.meetingModeOneWay;
  if (mode === "two_way") return t.meetingModeTwoWay;
  return t.meetingModeTranscription;
}

export function minutesHasContent(minutes: Minutes): boolean {
  return (
    minutes.summary.trim().length > 0 ||
    minutes.decisions.length > 0 ||
    minutes.next_actions.length > 0
  );
}

/** Drop LLM meta-chat/refusals — overlay must never paint these as subtitles. */
export function looksLikeTranslateRefusal(text: string): boolean {
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

export function usableTranslation(text: string | null | undefined): string | null {
  if (!text?.trim()) return null;
  if (looksLikeTranslateRefusal(text)) return null;
  return text.trim();
}

/** Group consecutive lines from the same speaker into readable turns. */
export function groupTurns(lines: TranscriptLine[]): TranscriptTurn[] {
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

/** Chronological milestone list — time-bucketed speaker clusters. */
export function buildTimeline(lines: TranscriptLine[]): TimelineStep[] {
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

export function translationPatchKey(ts: number, speaker: string | null | undefined): string {
  return `${ts}:${speaker ?? ""}`;
}
