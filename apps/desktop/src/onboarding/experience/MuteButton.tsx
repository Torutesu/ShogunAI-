import type { JSX } from "react";
import { t } from "../../strings";

export function MuteButton({ muted, disabled = false, onToggle }: { muted: boolean; disabled?: boolean; onToggle: () => Promise<boolean> }): JSX.Element {
  return (
    <button className="onb-mute" type="button" disabled={disabled} aria-pressed={muted} aria-label={muted ? t.onboarding.unmute : t.onboarding.mute} onClick={() => void onToggle()}>
      {muted ? t.onboarding.unmute : t.onboarding.mute}
    </button>
  );
}
