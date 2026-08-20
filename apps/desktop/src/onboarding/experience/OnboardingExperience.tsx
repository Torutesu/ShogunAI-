import { useCallback, useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { t } from "../../strings";
import { Logo } from "../../Logo";
import type { OnboardingState, PermissionSnapshot } from "../ipc";
import { ConnectStage, PlanStage, PrivacyStage } from "./FlowParity";
import { GateFrame } from "./GateFrame";
import { MuteButton } from "./MuteButton";
import { OverviewStage } from "./OverviewStage";
import { PermissionStage } from "./PermissionStage";
import { ShortcutPractice } from "./ShortcutPractice";
import { ThemeStage } from "./ThemeStage";

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
  const [gatePlaying, setGatePlaying] = useState(false);
  const finishStarted = useRef(false);
  const step = routeStep(state.step, permissions);
  const finishOnce = useCallback((): void => {
    if (finishStarted.current) return;
    finishStarted.current = true;
    void onFinish().then((saved) => {
      if (!saved) {
        finishStarted.current = false;
        setGatePlaying(false);
        setFinishing(false);
      }
    });
  }, [onFinish]);
  const finish = (): void => {
    if (finishing) return;
    setFinishing(true);
    const reducedMotion = typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
    if (reducedMotion) finishOnce();
    else setGatePlaying(true);
  };
  useEffect(() => {
    if (!gatePlaying) return;
    const fallback = window.setTimeout(finishOnce, 7000);
    return () => window.clearTimeout(fallback);
  }, [finishOnce, gatePlaying]);
  if (step === "welcome") {
    return (
      <main className="onb-shell onb-shell--welcome" data-step={step}>
        <WindowDragRegion />
        <Welcome onContinue={() => onPersist("reads")} />
        <div className="onb-floating-mute"><MuteButton muted={state.music_muted} disabled={musicPending} onToggle={onToggleMusic} /></div>
      </main>
    );
  }
  return (
    <main className="onb-shell" data-step={step}>
      <WindowDragRegion />
      <div className="onb-layout">
        <div className={`onb-copy${step === "overview" || step === "theme" ? " onb-copy--overview" : ""}`} key={step}>
          {step === "overview" ? <OverviewStage onBack={() => onPersist("welcome")} onContinue={() => onPersist("theme")} /> : null}
          {step === "theme" ? <ThemeStage onBack={() => onPersist("reads")} onContinue={() => onPersist("accessibility")} /> : null}
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
        <GateFrame complete={step === "gate" && gatePlaying} onEnded={finishOnce} onError={finishOnce} />
      </div>
      <div className="onb-floating-mute"><MuteButton muted={state.music_muted} disabled={musicPending} onToggle={onToggleMusic} /></div>
    </main>
  );
}

function WindowDragRegion(): JSX.Element {
  return <div className="onb-window-drag-region" data-tauri-drag-region aria-hidden="true" />;
}

function routeStep(step: OnboardingState["step"], p: PermissionSnapshot): "welcome" | "overview" | "theme" | "privacy" | "accessibility" | "microphone" | "screen_recording" | "right_option" | "scribe_demo" | "dictation_demo" | "plan" | "connect" | "gate" {
  if (step === "intro" || step === "welcome") return "welcome";
  if (step === "reads") return "overview";
  if (step === "theme") return "theme";
  if (step === "privacy") return "privacy";
  if (step === "permission") return !p.accessibility ? "accessibility" : !p.microphone ? "microphone" : !p.screen_recording ? "screen_recording" : "right_option";
  if (step === "accessibility") return "accessibility";
  if (step === "microphone" || step === "screen_recording" || step === "right_option" || step === "scribe_demo" || step === "dictation_demo" || step === "plan" || step === "connect" || step === "gate") return step;
  return "gate";
}

function Welcome({ onContinue }: { onContinue: () => Promise<boolean> }): JSX.Element {
  const [signingIn, setSigningIn] = useState(false);
  const timer = useRef<number | null>(null);
  useEffect(() => () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
  }, []);
  const signIn = (): void => {
    if (signingIn) return;
    setSigningIn(true);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      void onContinue().then((saved) => {
        if (!saved) setSigningIn(false);
      });
    }, 900);
  };
  return (
    <section className="onb-signin" data-testid="signin-welcome">
      <div className="onb-signin__haze" aria-hidden="true" />
      <div className="onb-signin__content">
        <Logo size={94} className="onb-signin__mark" />
        <h1>{t.onboarding.signInTitle}</h1>
        <p>{t.onboarding.signInLead}</p>
        <button className="onb-signin__button" type="button" disabled={signingIn} data-processing={signingIn} aria-busy={signingIn} onClick={signIn}>
          {signingIn ? <span className="onb-signin__spinner" aria-hidden="true" /> : null}
          <span className="onb-signin__label">{signingIn ? t.onboarding.signingIn : t.onboarding.signInBrowser}</span>
          {!signingIn ? <span className="onb-signin__external" aria-hidden="true">↗</span> : null}
        </button>
      </div>
    </section>
  );
}
