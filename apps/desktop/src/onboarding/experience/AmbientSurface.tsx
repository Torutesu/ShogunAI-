import type { JSX } from "react";
import wavesUrl from "../../assets/onboarding/icons/waves.svg";
import type { OnboardingMotionVector } from "../ipc";

function boundedDirection(value: number): -1 | 0 | 1 {
  return value > 0 ? 1 : value < 0 ? -1 : 0;
}

export function AmbientSurface(props: { motionVector: OnboardingMotionVector }): JSX.Element {
  const x = boundedDirection(props.motionVector.x);
  const y = boundedDirection(props.motionVector.y);
  return (
    <main className="onb-ambient" data-testid="ambient-surface" data-motion-x={x} data-motion-y={y} aria-hidden="true">
      <img src={wavesUrl} alt="" />
    </main>
  );
}
