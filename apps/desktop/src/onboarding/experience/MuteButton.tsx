import type { JSX } from "react";
import { t } from "../../strings";

export function MuteButton({ muted, disabled = false, onToggle }: { muted: boolean; disabled?: boolean; onToggle: () => Promise<boolean> }): JSX.Element {
  return (
    <button className="onb-mute" type="button" disabled={disabled} data-muted={muted} aria-pressed={muted} aria-label={muted ? t.onboarding.unmute : t.onboarding.mute} onClick={() => void onToggle()}>
      <svg className="onb-mute__icon" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M5 9.5v5h3.2l4.1 3.2V6.3L8.2 9.5H5Z" />
        {muted ? <path d="m16.2 9 4 6m0-6-4 6" /> : <><path d="M15.5 9.2a4 4 0 0 1 0 5.6" /><path d="M18 6.8a7.2 7.2 0 0 1 0 10.4" /></>}
      </svg>
    </button>
  );
}
