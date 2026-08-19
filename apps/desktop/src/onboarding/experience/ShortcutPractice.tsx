import { useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { listen } from "@tauri-apps/api/event";
import { comboChips, DEFAULT_BINDS } from "../../keys";
import { t } from "../../strings";
import { getShortcuts, onboardingShortcutArm, onboardingShortcutDisarm, onboardingShortcutReady } from "../ipc";
import type { OnboardingShortcutArm, OnboardingShortcutEvent, OnboardingState } from "../ipc";

export type PracticeKind = "right_option" | "scribe_demo" | "dictation_demo";

export function ShortcutPractice(props: {
  kind: PracticeKind;
  expectedRevision: number;
  surfaceGeneration: number;
  onPersist: (step: OnboardingState["step"]) => Promise<boolean>;
}): JSX.Element {
  const { kind, expectedRevision, surfaceGeneration, onPersist } = props;
  const [binds, setBinds] = useState<Record<string, string>>(DEFAULT_BINDS);
  const [held, setHeld] = useState(false);
  const [armed, setArmed] = useState<OnboardingShortcutArm | null>(null);
  const [sample, setSample] = useState<string>(t.onboarding.sampleEmail);
  const field = useRef<HTMLTextAreaElement | null>(null);
  const persisted = useRef(false);

  useEffect(() => { void getShortcuts().then((next) => setBinds((current) => ({ ...current, ...next }))); }, []);
  useEffect(() => {
    let alive = true;
    let arm: OnboardingShortcutArm | null = null;
    void onboardingShortcutArm(expectedRevision, kind).then((next) => {
      if (!alive || !next || next.stage !== kind) return;
      arm = next;
      setArmed(next);
      if (kind === "scribe_demo") {
        setSample(next.seeded_text || t.onboarding.sampleEmail);
        requestAnimationFrame(() => {
          if (!alive || !field.current) return;
          field.current.focus();
          field.current.select();
          void onboardingShortcutReady(next.generation, next.nonce, surfaceGeneration);
        });
      } else {
        void onboardingShortcutReady(next.generation, next.nonce, surfaceGeneration);
      }
    });
    const off = listen<OnboardingShortcutEvent>("onboarding-shortcut", (event) => {
      const outcome = event.payload;
      if (!arm || outcome.generation !== arm.generation || outcome.nonce !== arm.nonce || outcome.stage !== kind || persisted.current) return;
      if (kind === "right_option" && outcome.outcome === "single_tap") {
        persisted.current = true;
        void onPersist("scribe_demo").then((saved) => { if (!saved) persisted.current = false; });
      }
      if (kind === "scribe_demo" && outcome.outcome === "scribe_inserted" && outcome.session_id !== null) {
        persisted.current = true;
        void onPersist("dictation_demo").then((saved) => { if (!saved) persisted.current = false; });
      }
    });
    return () => {
      alive = false;
      void off.then((unlisten) => unlisten());
      if (arm) void onboardingShortcutDisarm(arm.generation, arm.nonce);
    };
  }, [expectedRevision, kind, onPersist, surfaceGeneration]);

  const binding = armed?.binding ?? (kind === "dictation_demo" ? binds.voice : binds.draft);
  const heading = kind === "right_option" ? t.onboarding.singleOptionTitle : kind === "scribe_demo" ? t.onboarding.doubleOptionTitle : t.onboarding.dictationTitle;
  const shortcut = comboChips(binding ?? (kind === "dictation_demo" ? DEFAULT_BINDS.voice : DEFAULT_BINDS.draft)).join(" + ");
  const defaultBinding = kind === "dictation_demo" ? DEFAULT_BINDS.voice : DEFAULT_BINDS.draft;
  const detail = binding === defaultBinding
    ? kind === "right_option" ? t.onboarding.singleOptionDetail : kind === "scribe_demo" ? t.onboarding.doubleOptionDetail : t.onboarding.dictationDetail
    : kind === "right_option" ? t.onboarding.singleOptionCustom.replace("{shortcut}", shortcut) : kind === "scribe_demo" ? t.onboarding.doubleOptionCustom.replace("{shortcut}", shortcut) : t.onboarding.dictationCustom.replace("{shortcut}", shortcut);
  return (
    <section className="onb-stage onb-stage--practice">
      <p className="onb-eyebrow">{t.onboarding.practiceStep}</p><h1>{heading}</h1><p className="onb-lead">{detail}</p>
      <div className="onb-keyline" aria-label={t.onboarding.shortcutLabel}>{shortcut.split(" + ").map((key, index) => <kbd key={`${key}-${index}`}>{key}</kbd>)}</div>
      {kind === "scribe_demo" ? <textarea ref={field} className="onb-practice-field" aria-label={t.onboarding.emailField} value={sample} onChange={(event) => setSample(event.target.value)} /> : kind === "dictation_demo" ? <textarea className="onb-practice-field" aria-label={t.onboarding.dictationField} placeholder={t.onboarding.dictationPlaceholder} onKeyDown={() => setHeld(true)} onKeyUp={() => setHeld(false)} /> : <div className="onb-practice-key" data-held={held}>{t.onboarding.rightOptionKey}</div>}
      <p className="onb-note">{kind === "dictation_demo" ? t.onboarding.dictationWaiting : t.onboarding.practiceWaiting}</p>
      <button className="onb-button onb-button--quiet" type="button" onClick={() => setHeld(false)}>{t.onboarding.tryAgain}</button>
    </section>
  );
}
