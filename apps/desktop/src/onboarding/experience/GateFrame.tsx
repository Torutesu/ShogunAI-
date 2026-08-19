import type { JSX } from "react";
import { MarkFacets } from "../../Logo";
import { t } from "../../strings";

export function GateFrame(props: { complete?: boolean; variant?: "frame" | "full-window" }): JSX.Element {
  const { complete = false, variant = "frame" } = props;
  return (
    <aside className={`onb-gate onb-gate--${variant}`} data-testid="gate-frame" data-complete={complete}>
      <div className="onb-gate__rule" />
      <div className="onb-gate__media" aria-label={complete ? t.onboarding.gateOpen : t.onboarding.gateWaiting}>
        <svg className="onb-gate__mark" viewBox="0 0 957 614" aria-hidden="true">
          <MarkFacets />
        </svg>
        <span className="onb-gate__caption">{complete ? t.onboarding.gateOpen : t.onboarding.gateWaiting}</span>
      </div>
      <p>{t.onboarding.gateDetail}</p>
    </aside>
  );
}
