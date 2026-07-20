'use client';

import { useEffect } from 'react';
import { playClick, playHover, soundEnabled } from '@/lib/sound';

/**
 * Mounts once and delegates hover/click sounds to every interactive element.
 * No-ops while sound is disabled (the common case), so it's cheap to keep on.
 * Hover ticks are throttled and mouse-only; touch just gets click sounds.
 */
export function SoundInteractions() {
  useEffect(() => {
    let lastHover = 0;
    const interactive = (t: EventTarget | null) =>
      t instanceof Element ? t.closest('a,button,[role="button"]') : null;

    const onOver = (e: PointerEvent) => {
      if (!soundEnabled() || e.pointerType !== 'mouse' || !interactive(e.target)) return;
      const now = performance.now();
      if (now - lastHover < 70) return; // debounce rapid boundary crossings
      lastHover = now;
      playHover();
    };
    const onDown = (e: PointerEvent) => {
      if (!soundEnabled() || !interactive(e.target)) return;
      playClick();
    };

    document.addEventListener('pointerover', onOver, { passive: true });
    document.addEventListener('pointerdown', onDown, { passive: true });
    return () => {
      document.removeEventListener('pointerover', onOver);
      document.removeEventListener('pointerdown', onDown);
    };
  }, []);

  return null;
}
