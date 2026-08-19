import type { JSX } from "react";
import gateImageUrl from "../../assets/onboarding/gate-autumn-path.png";
import { t } from "../../strings";

export function GateFrame(props: { complete?: boolean; variant?: "frame" | "full-window" }): JSX.Element {
  const { complete = false, variant = "frame" } = props;
  return (
    <aside className={`onb-gate onb-gate--${variant}`} data-testid="gate-frame" data-complete={complete}>
      <div className="onb-gate__picture">
        <img
          className="onb-gate__image"
          src={gateImageUrl}
          alt={t.onboarding.gateAlt}
          width="1024"
          height="1536"
          fetchPriority="high"
        />
      </div>
      <div className="onb-gate__legend">
        <span className="onb-gate__caption">{complete ? t.onboarding.gateOpen : t.onboarding.gateWaiting}</span>
        <p>{t.onboarding.gateDetail}</p>
      </div>
    </aside>
  );
}
