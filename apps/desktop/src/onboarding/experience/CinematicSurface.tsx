import type { JSX } from "react";
import wavesUrl from "../../assets/onboarding/icons/waves.svg";
import { Logo } from "../../Logo";
import { MuteButton } from "./MuteButton";

export function CinematicSurface({ muted, musicPending, onToggleMusic }: { muted: boolean; musicPending: boolean; onToggleMusic: () => Promise<boolean> }): JSX.Element {
  return (
    <main className="onb-cinematic" data-testid="cinematic-surface">
      <img className="onb-cinematic__wave onb-cinematic__wave--one" src={wavesUrl} alt="" />
      <img className="onb-cinematic__wave onb-cinematic__wave--two" src={wavesUrl} alt="" />
      <Logo size={92} className="onb-cinematic__mark" />
      <MuteButton muted={muted} disabled={musicPending} onToggle={onToggleMusic} />
    </main>
  );
}
