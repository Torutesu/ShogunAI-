import { useState } from "react";
import type { JSX } from "react";
import { Logo } from "../../Logo";
import { t } from "../../strings";
import type { OnboardingState, PermissionSnapshot } from "../ipc";
import { ConnectStage, PlanStage, PrivacyStage } from "./FlowParity";
import { GateFrame } from "./GateFrame";
import { MuteButton } from "./MuteButton";
import { PermissionStage } from "./PermissionStage";
import { ShortcutPractice } from "./ShortcutPractice";

export function OnboardingExperience(props: {
  state: OnboardingState;
  permissions: PermissionSnapshot;
  surfaceGeneration: number;
  onPersist: (step: OnboardingState["step"], patch?: Partial<OnboardingState>) => Promise<boolean>;
  onFinish: () => Promise<boolean>;
  onToggleMusic: () => Promise<boolean>;
  musicPending: boolean;
}): JSX.Element {
  const { state, permissions, surfaceGeneration, onPersist, onFinish, onToggleMusic, musicPending } = props;
  const [finishing, setFinishing] = useState(false);
  const step = routeStep(state.step, permissions);
  const finish = (): void => {
    setFinishing(true);
    void onFinish().then((saved) => { if (!saved) setFinishing(false); });
  };
  return (
    <main className="onb-shell" data-step={step}>
      <header className="onb-header"><Logo size={26} /><span>{t.onboarding.brand}</span><MuteButton muted={state.music_muted} disabled={musicPending} onToggle={onToggleMusic} /></header>
      <div className="onb-layout">
        <div className="onb-copy" key={step}>
          {step === "welcome" ? <Welcome onContinue={() => onPersist("accessibility")} /> : null}
          {step === "privacy" ? <PrivacyStage onContinue={() => onPersist("plan")} /> : null}
          {step === "accessibility" || step === "microphone" || step === "screen_recording" ? (
            <PermissionStage kind={step} permissions={permissions} state={state} onPersist={onPersist} />
          ) : null}
          {step === "right_option" || step === "scribe_demo" || step === "dictation_demo" ? <ShortcutPractice kind={step} expectedRevision={state.revision} surfaceGeneration={surfaceGeneration} onPersist={onPersist} /> : null}
          {step === "plan" ? <PlanStage state={state} onPersist={onPersist} /> : null}
          {step === "connect" ? <ConnectStage onPersist={onPersist} /> : null}
          {step === "gate" ? (
            <section className="onb-stage">
              <p className="onb-eyebrow">{t.onboarding.readyStep}</p><h1>{t.onboarding.gateTitle}</h1><p className="onb-lead">{t.onboarding.gateLead}</p>
              <button className="onb-button onb-button--primary" type="button" disabled={finishing} onClick={finish}>{t.onboarding.continue}</button>
            </section>
          ) : null}
        </div>
        <GateFrame complete={step === "gate" && finishing} />
      </div>
    </main>
  );
}

function routeStep(step: OnboardingState["step"], p: PermissionSnapshot): "welcome" | "privacy" | "accessibility" | "microphone" | "screen_recording" | "right_option" | "scribe_demo" | "dictation_demo" | "plan" | "connect" | "gate" {
  if (step === "intro" || step === "welcome") return "welcome";
  if (step === "reads" || step === "privacy") return "privacy";
  if (step === "permission") return !p.accessibility ? "accessibility" : !p.microphone ? "microphone" : !p.screen_recording ? "screen_recording" : "right_option";
  if (step === "accessibility") return "accessibility";
  if (step === "microphone" || step === "screen_recording" || step === "right_option" || step === "scribe_demo" || step === "dictation_demo" || step === "plan" || step === "connect" || step === "gate") return step;
  return "gate";
}

function Welcome({ onContinue }: { onContinue: () => Promise<boolean> }): JSX.Element {
  return <section className="onb-stage"><p className="onb-eyebrow">{t.onboarding.welcomeStep}</p><h1>{t.onboarding.welcomeTitle}</h1><p className="onb-lead">{t.onboarding.welcomeLead}</p><button className="onb-button onb-button--primary" type="button" onClick={() => void onContinue()}>{t.onboarding.continue}</button></section>;
}
