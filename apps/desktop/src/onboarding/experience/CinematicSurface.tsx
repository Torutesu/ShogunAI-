import type { JSX } from "react";

export function CinematicSurface(): JSX.Element {
  return (
    <main className="onb-cinematic" data-testid="cinematic-surface">
      <div className="onb-cinematic__light onb-cinematic__light--ember" aria-hidden="true" />
      <div className="onb-cinematic__light onb-cinematic__light--glacier" aria-hidden="true" />
      <div className="onb-cinematic__bloom" aria-hidden="true" />
      <div className="onb-cinematic__window" data-testid="cinematic-window-form" aria-hidden="true" />
    </main>
  );
}
