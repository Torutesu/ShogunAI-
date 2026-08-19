import { useEffect, useState } from "react";
import type { JSX } from "react";
import { comboChips, DEFAULT_BINDS } from "../../keys";
import { t } from "../../strings";
import { getShortcuts } from "../ipc";

export type PracticeKind = "right_option" | "scribe_demo" | "dictation_demo";

const sampleEmail = "hi team, can we move our review to friday morning? i can share notes after";

export function ShortcutPractice({ kind }: { kind: PracticeKind }): JSX.Element {
  const [binds, setBinds] = useState<Record<string, string>>(DEFAULT_BINDS);
  const [held, setHeld] = useState(false);
  useEffect(() => { void getShortcuts().then((next) => setBinds((current) => ({ ...current, ...next }))); }, []);

  const binding = kind === "dictation_demo" ? binds.voice : binds.draft;
  const heading = kind === "right_option" ? t.onboarding.singleOptionTitle : kind === "scribe_demo" ? t.onboarding.doubleOptionTitle : t.onboarding.dictationTitle;
  const detail = kind === "right_option" ? t.onboarding.singleOptionDetail : kind === "scribe_demo" ? t.onboarding.doubleOptionDetail : t.onboarding.dictationDetail;
  return (
    <section className="onb-stage onb-stage--practice">
      <p className="onb-eyebrow">{t.onboarding.practiceStep}</p>
      <h1>{heading}</h1>
      <p className="onb-lead">{detail}</p>
      <div className="onb-keyline" aria-label={t.onboarding.shortcutLabel}>
        {comboChips(binding ?? (kind === "dictation_demo" ? DEFAULT_BINDS.voice : DEFAULT_BINDS.draft)).map((key, index) => <kbd key={`${key}-${index}`}>{key}</kbd>)}
      </div>
      {kind === "scribe_demo" ? (
        <textarea className="onb-practice-field" aria-label={t.onboarding.emailField} defaultValue={sampleEmail} />
      ) : kind === "dictation_demo" ? (
        <textarea
          className="onb-practice-field"
          aria-label={t.onboarding.dictationField}
          placeholder={t.onboarding.dictationPlaceholder}
          onKeyDown={() => setHeld(true)}
          onKeyUp={() => setHeld(false)}
        />
      ) : <div className="onb-practice-key" data-held={held}>{t.onboarding.rightOptionKey}</div>}
      <p className="onb-note">{kind === "dictation_demo" ? t.onboarding.dictationWaiting : t.onboarding.practiceWaiting}</p>
      <button className="onb-button onb-button--quiet" type="button" onClick={() => setHeld(false)}>{t.onboarding.tryAgain}</button>
    </section>
  );
}
