import { useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { listen } from "@tauri-apps/api/event";
import { comboChips, DEFAULT_BINDS } from "../../keys";
import { t } from "../../strings";
import { enableOnboardingDictation, getShortcuts, onboardingShortcutArm, onboardingShortcutDisarm, onboardingShortcutReady, restoreOnboardingShortcut } from "../ipc";
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
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement | null>(null);
  const persisted = useRef(false);

  useEffect(() => { void getShortcuts().then((next) => setBinds((current) => ({ ...current, ...next }))); }, []);
  useEffect(() => {
    let alive = true;
    let arm: OnboardingShortcutArm | null = null;
    let unlisten: (() => void) | null = null;
    setArmed(null);
    void listen<OnboardingShortcutEvent>("onboarding-shortcut", (event) => {
      const outcome = event.payload;
      if (!arm || outcome.generation !== arm.generation || outcome.nonce !== arm.nonce || outcome.stage !== kind || persisted.current) return;
      if (["no_key", "failed", "cancelled", "stale", "dictation_copied"].includes(outcome.outcome)) setResult(outcome.outcome);
      if (kind === "right_option" && outcome.outcome === "single_tap") {
        persisted.current = true;
        void onPersist("scribe_demo").then((saved) => { if (!saved) persisted.current = false; });
      }
      if (kind === "scribe_demo" && outcome.outcome === "scribe_inserted" && outcome.session_id !== null) {
        persisted.current = true;
        void onPersist("dictation_demo").then((saved) => { if (!saved) persisted.current = false; });
      }
      if (kind === "dictation_demo" && outcome.outcome === "dictation_inserted" && outcome.session_id !== null) {
        persisted.current = true;
        void onPersist("plan").then((saved) => { if (!saved) persisted.current = false; });
      }
    }).then((off) => {
      if (!alive) { off(); return; }
      unlisten = off;
      return onboardingShortcutArm(expectedRevision, kind).then((next) => {
        if (!next || next.stage !== kind) return;
        if (!alive) {
          void onboardingShortcutDisarm(next.generation, next.nonce);
          return;
        }
        arm = next;
        setArmed(next);
        const supportsDemo = next.supports_demo ?? (kind !== "scribe_demo" || next.supports_scribe !== false);
        if (!supportsDemo || (kind === "dictation_demo" && next.voice_enabled === false)) return;
        if (kind === "scribe_demo" || kind === "dictation_demo") {
          requestAnimationFrame(() => {
            if (!alive || !field.current) return;
            field.current.focus();
            if (kind === "scribe_demo") field.current.select();
            else field.current.setSelectionRange(field.current.value.length, field.current.value.length);
            void onboardingShortcutReady(next.generation, next.nonce, surfaceGeneration);
          });
        } else {
          void onboardingShortcutReady(next.generation, next.nonce, surfaceGeneration);
        }
      });
    });
    return () => {
      alive = false;
      unlisten?.();
      if (arm) void onboardingShortcutDisarm(arm.generation, arm.nonce);
    };
  }, [attempt, expectedRevision, kind, onPersist, surfaceGeneration]);

  const binding = armed?.binding ?? (kind === "dictation_demo" ? binds.voice : binds.draft);
  const shortcut = comboChips(binding ?? (kind === "dictation_demo" ? DEFAULT_BINDS.voice : DEFAULT_BINDS.draft)).join(" + ");
  const defaultBinding = kind === "dictation_demo" ? DEFAULT_BINDS.voice : DEFAULT_BINDS.draft;
  const supportsDemo = armed?.supports_demo ?? (kind !== "scribe_demo" || armed?.supports_scribe !== false);
  const heading = binding === defaultBinding
    ? kind === "right_option" ? t.onboarding.singleOptionTitle : kind === "scribe_demo" ? t.onboarding.doubleOptionTitle : t.onboarding.dictationTitle
    : kind === "right_option" ? t.onboarding.singleOptionCustomTitle.replace("{shortcut}", shortcut) : kind === "scribe_demo" ? t.onboarding.doubleOptionCustomTitle.replace("{shortcut}", shortcut) : t.onboarding.dictationCustomTitle.replace("{shortcut}", shortcut);
  const detail = binding === defaultBinding
    ? kind === "right_option" ? t.onboarding.singleOptionDetail : kind === "scribe_demo" ? t.onboarding.doubleOptionDetail : t.onboarding.dictationDetail
    : kind === "right_option" ? t.onboarding.singleOptionCustom.replace("{shortcut}", shortcut) : kind === "scribe_demo" ? t.onboarding.doubleOptionCustom.replace("{shortcut}", shortcut) : t.onboarding.dictationCustom.replace("{shortcut}", shortcut);
  const retry = (): void => {
    persisted.current = false;
    setHeld(false);
    setResult(null);
    setAttempt((value) => value + 1);
  };
  const restore = (): void => {
    const action = kind === "dictation_demo" ? "voice" : "draft";
    void restoreOnboardingShortcut(action, defaultBinding).then((saved) => { if (saved) retry(); });
  };
  return (
    <section className="onb-stage onb-stage--practice">
      <p className="onb-eyebrow">{t.onboarding.practiceStep}</p><h1>{heading}</h1><p className="onb-lead">{detail}</p>
      <div className="onb-keyline" aria-label={t.onboarding.shortcutLabel}>{shortcut.split(" + ").map((key, index) => <kbd key={`${key}-${index}`}>{key}</kbd>)}</div>
      {kind === "scribe_demo" ? <textarea key={armed?.generation ?? "loading"} ref={field} className="onb-practice-field" aria-label={t.onboarding.emailField} defaultValue={armed?.seeded_text || t.onboarding.sampleEmail} /> : kind === "dictation_demo" ? <textarea ref={field} className="onb-practice-field" aria-label={t.onboarding.dictationField} placeholder={t.onboarding.dictationPlaceholder} onKeyDown={() => setHeld(true)} onKeyUp={() => setHeld(false)} /> : <div className="onb-practice-key" data-held={held}>{shortcut}</div>}
      <p className="onb-note">{result ? t.onboarding.practiceRetry : kind === "dictation_demo" ? t.onboarding.dictationWaiting : t.onboarding.practiceWaiting}</p>
      {!supportsDemo && armed ? <button className="onb-button onb-button--quiet" type="button" onClick={restore}>{kind === "dictation_demo" ? t.onboarding.restoreDictation : t.onboarding.restoreRightOption}</button> : null}
      {kind === "dictation_demo" && armed?.voice_enabled === false ? <button className="onb-button onb-button--quiet" type="button" onClick={() => { void enableOnboardingDictation().then((enabled) => { if (enabled) retry(); }); }}>{t.onboarding.enableDictation}</button> : null}
      <button className="onb-button onb-button--quiet" type="button" onClick={retry}>{t.onboarding.tryAgain}</button>
    </section>
  );
}
