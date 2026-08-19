import type { JSX } from "react";
import { t } from "../../strings";

/** Native audio ownership lands separately. Never pretend a webview toggle controls playback. */
export function MuteButton({ muted }: { muted: boolean }): JSX.Element {
  return (
    <button className="onb-mute" type="button" disabled aria-label={muted ? t.onboarding.unmute : t.onboarding.mute}>
      {muted ? t.onboarding.unmute : t.onboarding.mute}
    </button>
  );
}
