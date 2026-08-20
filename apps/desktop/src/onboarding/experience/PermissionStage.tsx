import { useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import appIconUrl from "../../../src-tauri/icons/icon-128.png";
import { t } from "../../strings";
import {
  armPermissionDrag,
  disarmPermissionDrag,
  requestAxPermission,
  requestMicrophonePermission,
  requestScreenRecordingPermission,
  restartOnboarding,
  track,
} from "../ipc";
import type { OnboardingState, PermissionSnapshot } from "../ipc";

export type PermissionStageKind = "accessibility" | "microphone" | "screen_recording";

const permissionCopy = {
  accessibility: {
    title: t.onboarding.accessibilityTitle,
    detail: t.onboarding.accessibilityDetail,
    settings: t.onboarding.openSettings,
  },
  microphone: {
    title: t.onboarding.microphoneTitle,
    detail: t.onboarding.microphoneDetail,
    settings: t.onboarding.openSettings,
  },
  screen_recording: {
    title: t.onboarding.screenTitle,
    detail: t.onboarding.screenDetail,
    settings: t.onboarding.openSettings,
  },
} as const;

export function PermissionStage(props: {
  kind: PermissionStageKind;
  permissions: PermissionSnapshot;
  state: OnboardingState;
  onPersist: (step: OnboardingState["step"]) => Promise<boolean>;
}): JSX.Element {
  const { kind, permissions, state, onPersist } = props;
  const [requested, setRequested] = useState(false);
  const [restartFailed, setRestartFailed] = useState(false);
  const advanced = useRef<PermissionStageKind | null>(null);
  const copy = permissionCopy[kind];
  const granted = kind === "accessibility"
    ? permissions.accessibility
    : kind === "microphone"
      ? permissions.microphone
      : permissions.screen_recording;
  const restartRequired = kind === "screen_recording" && permissions.screen_recording_state === "restart_required";
  const next = kind === "accessibility" ? "microphone" : kind === "microphone" ? "screen_recording" : "right_option";

  useEffect(() => {
    if (!granted || advanced.current === kind) return;
    advanced.current = kind;
    void onPersist(next).then((saved) => {
      if (!saved && advanced.current === kind) advanced.current = null;
    });
  }, [granted, kind, next, onPersist]);

  const request = (): void => {
    setRequested(true);
    track(`${kind}_requested`);
    if (kind === "accessibility") void requestAxPermission();
    else if (kind === "microphone") void requestMicrophonePermission();
    else void requestScreenRecordingPermission();
  };
  const restart = (): void => {
    setRestartFailed(false);
    void restartOnboarding(state).catch(() => setRestartFailed(true));
  };

  return (
    <section className="onb-stage onb-stage--permission" aria-live="polite">
      <p className="onb-eyebrow">{t.onboarding.permissionStep}</p>
      <h1>{copy.title}</h1>
      <p className="onb-lead">{copy.detail}</p>
      <div className="onb-status" data-ready={granted}>
        <span className="onb-status__dot" />
        {granted ? t.onboarding.permissionReady : restartRequired ? t.onboarding.restartNeeded : t.onboarding.waiting}
      </div>
      {restartRequired ? (
        <div className="onb-actions">
          <button className="onb-button onb-button--primary" type="button" onClick={restart}>{t.onboarding.restart}</button>
          {restartFailed ? <p className="onb-recovery">{t.onboarding.restartFailed}</p> : null}
        </div>
      ) : (
        <div className="onb-actions">
          <button className="onb-button onb-button--primary" type="button" onClick={request}>
            {requested || (kind === "microphone" && permissions.microphone_state !== "not_determined") ? copy.settings : t.onboarding.allow}
          </button>
          {kind !== "microphone" && requested && !granted ? <PermissionDrag title={copy.title} onOpen={request} /> : null}
        </div>
      )}
      <p className="onb-note">{t.onboarding.permissionPrivacy}</p>
    </section>
  );
}

function PermissionDrag({ title, onOpen }: { title: string; onOpen: () => void }): JSX.Element {
  return (
    <button
      className="onb-drag"
      type="button"
      onPointerEnter={() => void armPermissionDrag()}
      onPointerLeave={(event) => { if (event.buttons === 0) void disarmPermissionDrag(); }}
      onPointerDown={(event) => { if (event.button === 0) void armPermissionDrag(); }}
      onClick={onOpen}
    >
      <img src={appIconUrl} alt="" draggable={false} />
      <span>{t.onboarding.dragHelper.replace("{permission}", title)}</span>
    </button>
  );
}
