'use client';

import { Volume2, VolumeX } from 'lucide-react';
import { useEffect, useState } from 'react';
import { initSound, setSound } from '@/lib/sound';

/** 🔊/🔇 opt-in toggle. Off by default; persists the choice. */
export function SoundToggle({ label }: { label?: string }) {
  const [on, setOn] = useState(false);

  useEffect(() => {
    setOn(initSound());
  }, []);

  return (
    <button
      type="button"
      aria-label={label ?? (on ? 'Mute interface sounds' : 'Enable interface sounds')}
      aria-pressed={on}
      onClick={() => {
        const next = !on;
        setOn(next);
        setSound(next); // enabling resumes the AudioContext + plays a confirm blip
      }}
      className="grid size-9 place-items-center rounded-full text-muted transition-colors hover:bg-cloud hover:text-ink"
    >
      {on ? <Volume2 className="size-[18px]" strokeWidth={1.9} /> : <VolumeX className="size-[18px]" strokeWidth={1.9} />}
    </button>
  );
}
