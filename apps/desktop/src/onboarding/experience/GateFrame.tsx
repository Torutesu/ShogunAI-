import type { JSX } from "react";
import gateImageUrl from "../../assets/onboarding/gate-autumn-path.png";
import gateVideoUrl from "../../assets/onboarding/gate-opening.mp4";
import { t } from "../../strings";

export function GateFrame(props: { complete?: boolean; variant?: "frame" | "full-window" }): JSX.Element {
  const { complete = false, variant = "frame" } = props;
  const reducedMotion = typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
  return (
    <aside className={`onb-gate onb-gate--${variant}`} data-testid="gate-frame" data-complete={complete}>
      <div className="onb-gate__picture">
        <img
          className="onb-gate__image"
          src={gateImageUrl}
          alt={t.onboarding.gateAlt}
          width="1024"
          height="1536"
        />
        {complete && !reducedMotion ? (
          <video className="onb-gate__video" data-testid="gate-opening-video" autoPlay muted playsInline preload="auto" poster={gateImageUrl} aria-hidden="true">
            <source src={gateVideoUrl} type="video/mp4" />
          </video>
        ) : null}
      </div>
    </aside>
  );
}
