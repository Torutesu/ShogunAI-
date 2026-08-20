import type { JSX } from "react";
import { MuteButton } from "./MuteButton";

export function CinematicSurface({ muted, musicPending, onToggleMusic }: { muted: boolean; musicPending: boolean; onToggleMusic: () => Promise<boolean> }): JSX.Element {
  return (
    <main className="onb-cinematic" data-testid="cinematic-surface">
      <div className="onb-cinematic__light onb-cinematic__light--ember" aria-hidden="true" />
      <div className="onb-cinematic__light onb-cinematic__light--cedar" aria-hidden="true" />
      <div className="onb-cinematic__light onb-cinematic__light--glacier" aria-hidden="true" />
      <div className="onb-cinematic__bloom" aria-hidden="true" />
      <MuteButton muted={muted} disabled={musicPending} onToggle={onToggleMusic} />
    </main>
  );
}
