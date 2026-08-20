import { useCallback, useEffect, useRef, useState } from "react";

import { translationPatchKey, usableTranslation, type TranscriptLine } from "./transforms";

/** Batches live lines, interim replacements, and late translation patches to animation frames. */
export function useLiveLineBuffer(active: boolean): {
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
      setInterims(interimBatch);
      return;
    }
    setInterims(interimBatch);
    if (batch.length === 0 && patches.size === 0) return;
    setLines((prev) => {
      let next = batch.length > 0 ? [...prev, ...batch] : prev;
      if (patches.size > 0) {
        next = next.map((line) => {
          const translation = patches.get(translationPatchKey(line.ts, line.speaker));
          return translation != null ? { ...line, translation } : line;
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

  const pushLine = useCallback((line: TranscriptLine) => {
    interimRef.current.delete(line.speaker ?? "");
    pendingRef.current.push(line);
    schedule();
  }, [schedule]);

  const upsertInterim = useCallback((line: TranscriptLine) => {
    interimRef.current.set(line.speaker ?? "", line);
    schedule();
  }, [schedule]);

  const patchTranslation = useCallback(
    (ts: number, speaker: string | null | undefined, translation: string) => {
      const usable = usableTranslation(translation);
      if (!usable) return;
      transRef.current.set(translationPatchKey(ts, speaker), usable);
      schedule();
    },
    [schedule],
  );

  const snapshot = useCallback((): TranscriptLine[] => [...lines, ...pendingRef.current], [lines]);

  return { lines, interims, pushLine, upsertInterim, patchTranslation, snapshot };
}
