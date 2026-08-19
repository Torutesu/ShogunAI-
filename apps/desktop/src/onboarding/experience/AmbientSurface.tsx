import type { JSX } from "react";
import wavesUrl from "../../assets/onboarding/icons/waves.svg";

export function AmbientSurface(): JSX.Element {
  return (
    <main className="onb-ambient" data-testid="ambient-surface" aria-hidden="true">
      <img src={wavesUrl} alt="" />
    </main>
  );
}
