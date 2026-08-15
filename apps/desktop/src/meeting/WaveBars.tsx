// The five waveform bars in the recording pill (#122).
//
// The previous shape kept the level in overlay-component state and ran a requestAnimationFrame
// loop for the idle pulse — every frame of an *idle* meeting re-rendered the largest component in
// the app, for a five-bar glyph. Two changes fix that structurally:
//
// - The idle pulse is CSS (`ov-wave-idle` keyframes, already in styles.css). No JS runs at all
//   while nobody is speaking.
// - Real levels bypass React entirely: the `meeting_level` listener writes each bar's `--h`
//   custom property straight onto the DOM nodes. Zero `setState`, zero re-renders, at any event
//   rate the Rust side chooses.
//
// The peak-decay normalisation and the per-bar shape formula are unchanged from the overlay's
// original math, so the glyph looks the same — it just costs nothing.

import { useEffect, useRef, type JSX } from "react";
import { listen } from "@tauri-apps/api/event";

/** Base amplitude per bar — the glyph's resting silhouette. */
const BAR_BASES = [0.35, 0.85, 0.55, 0.95, 0.45] as const;

/** No `meeting_level` for this long → clear inline heights so the CSS idle pulse shows again. */
const SILENCE_FALLBACK_MS = 1_500;

/** The bar-height formula the overlay has always used, per bar, for a normalised level 0..1. */
export function barHeightPct(base: number, index: number, level: number): number {
  const wobble = 0.15 * Math.sin(level * 12 + index * 1.7);
  const h = Math.max(0.18, Math.min(1, base * (0.35 + level * 0.9) + wobble));
  return Math.round(h * 100);
}

export function WaveBars({ active }: { active: boolean }): JSX.Element {
  const wrapRef = useRef<HTMLSpanElement | null>(null);
  const peakRef = useRef(0);
  const silenceTimerRef = useRef<number | null>(null);

  useEffect(() => {
    const clearHeights = (): void => {
      const wrap = wrapRef.current;
      if (!wrap) return;
      for (const bar of Array.from(wrap.children)) {
        (bar as HTMLElement).style.removeProperty("--h");
      }
    };
    if (!active) {
      peakRef.current = 0;
      clearHeights();
      return;
    }
    const off = listen<{ rms: number }>("meeting_level", (e) => {
      const wrap = wrapRef.current;
      if (!wrap) return;
      const rms = e.payload.rms;
      peakRef.current = Math.max(peakRef.current * 0.85, rms);
      const norm = peakRef.current > 0 ? Math.min(1, rms / peakRef.current) : 0;
      const bars = wrap.children;
      for (let i = 0; i < bars.length; i++) {
        (bars[i] as HTMLElement).style.setProperty(
          "--h",
          `${barHeightPct(BAR_BASES[i] ?? 0.5, i, norm)}%`,
        );
      }
      // Speech stopped → hand the glyph back to the CSS idle pulse instead of freezing on the
      // last real level.
      if (silenceTimerRef.current !== null) window.clearTimeout(silenceTimerRef.current);
      silenceTimerRef.current = window.setTimeout(() => {
        silenceTimerRef.current = null;
        clearHeights();
      }, SILENCE_FALLBACK_MS);
    });
    return () => {
      if (silenceTimerRef.current !== null) {
        window.clearTimeout(silenceTimerRef.current);
        silenceTimerRef.current = null;
      }
      clearHeights();
      void off.then((f) => f());
    };
  }, [active]);

  return (
    <span ref={wrapRef} className="ov__wave-glyph ov__wave-glyph--bars" aria-hidden>
      {BAR_BASES.map((_, i) => (
        <span key={i} className="ov__wave-bar" />
      ))}
    </span>
  );
}
