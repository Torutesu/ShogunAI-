import { useEffect, useState } from "react";
import type { JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnalyticsToggle } from "../../AnalyticsToggle";
import { ConnectionsList } from "../../connections";
import { count, t } from "../../strings";
import { exclusionCategories, IN_TAURI, openOnboardingSettings } from "../ipc";
import type { ExclusionCategory, OnboardingState } from "../ipc";

type Persist = (step: OnboardingState["step"], patch?: Partial<OnboardingState>) => Promise<boolean>;

export function PrivacyStage({ onContinue }: { onContinue: () => Promise<boolean> }): JSX.Element {
  const [categories, setCategories] = useState<ExclusionCategory[]>([]);
  useEffect(() => { void exclusionCategories().then(setCategories); }, []);
  const facts = [
    [t.obReadsKeep1Title, t.obReadsKeep1Body],
    [t.obReadsKeep2Title, t.obReadsKeep2Body],
    [t.obReadsKeep3Title, t.obReadsKeep3Body],
  ] as const;
  return (
    <section className="onb-stage onb-stage--privacy">
      <p className="onb-eyebrow">{t.onboarding.privacyStep}</p>
      <h1>{t.obReadsTitle}</h1>
      <p className="onb-lead">{t.obReadsBody}</p>
      <div className="onb-facts">
        {facts.map(([title, detail]) => <div className="onb-fact" key={title}><strong>{title}</strong><span>{detail}</span></div>)}
      </div>
      <div className="onb-exclusions">
        <strong>{t.obNeverTitle}</strong><p>{t.obNeverBody}</p>
        <div className="onb-exclusions__list">
          {categories.map((category) => <span key={category.id}>{t.obExclusion[category.id] ?? category.id} <b>{count(category.id === "sensitive_titles" ? t.obNeverRules : t.obNeverApps, category.count)}</b></span>)}
        </div>
      </div>
      <button className="onb-button onb-button--primary" type="button" onClick={() => void onContinue()}>{t.onboarding.continue}</button>
    </section>
  );
}

export function PlanStage({ state, onPersist }: { state: OnboardingState; onPersist: Persist }): JSX.Element {
  const [key, setKey] = useState("");
  const [saved, setSaved] = useState(false);
  const saveKey = (): void => {
    const value = key.trim();
    if (!value) return;
    if (!IN_TAURI) { setSaved(true); setKey(""); return; }
    void invoke("set_byok_key", { provider: "anthropic", key: value }).then(() => { setSaved(true); setKey(""); }).catch(() => undefined);
  };
  return (
    <section className="onb-stage">
      <p className="onb-eyebrow">{t.onboarding.planStep}</p><h1>{t.obPlanTitle}</h1><p className="onb-lead">{t.obPlanBody}</p>
      <div className="onb-plan-options">
        {(["standard", "pro"] as const).map((plan) => <button className={`onb-plan${state.plan === plan ? " is-selected" : ""}`} type="button" key={plan} aria-pressed={state.plan === plan} onClick={() => void onPersist("plan", { plan })}><strong>{plan === "standard" ? t.obPlanStandard : t.obPlanPro}</strong><span>{plan === "standard" ? t.obPlanStandardBody : t.obPlanProBody}</span></button>)}
      </div>
      <div className="onb-fact"><strong>{t.obPlanKeys}</strong><span>{t.obPlanKeysBody}</span></div>
      {state.plan === "pro" ? <div className="onb-fact"><strong>{t.obKeyTitle}</strong><span>{saved ? t.keySaved : t.obKeyBody}</span>{!saved ? <div className="onb-keyrow"><input type="password" aria-label={t.obKeyTitle} autoComplete="off" value={key} placeholder={t.keyPlaceholders.anthropic} onChange={(event) => setKey(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") saveKey(); }} /><button className="onb-button" type="button" disabled={!key.trim()} onClick={saveKey}>{t.keySave}</button></div> : null}</div> : null}
      <div className="onb-actions"><button className="onb-button onb-button--quiet" type="button" onClick={() => void onPersist("connect")}>{t.obSkip}</button><button className="onb-button onb-button--primary" type="button" onClick={() => void onPersist("connect")}>{t.onboarding.continue}</button></div>
    </section>
  );
}

export function ConnectStage({ onPersist }: { onPersist: Persist }): JSX.Element {
  return (
    <section className="onb-stage">
      <p className="onb-eyebrow">{t.onboarding.connectStep}</p><h1>{t.obConnectTitle}</h1><p className="onb-lead">{t.obConnectBody}</p>
      <div className="onb-draft-stop" role="status" aria-label={`${t.obDraftStop}, ${t.obDraftStopStatus}`}>
        <span className="onb-draft-stop__lock" aria-hidden="true">{t.obDraftStopStatus}</span>
        <span><strong>{t.obDraftStop}</strong><small>{t.obDraftStopBody}</small><small className="onb-locked">{t.obDraftStopLocked}</small><button className="onb-inline-action" type="button" onClick={() => void openOnboardingSettings()}>{t.onboarding.openSettings}</button></span>
      </div>
      <div className="onb-connections"><ConnectionsList connectableOnly /></div><div className="onb-analytics"><AnalyticsToggle /></div><p className="onb-note">{t.obConnectSkip}</p>
      <div className="onb-actions"><button className="onb-button onb-button--quiet" type="button" onClick={() => void onPersist("gate")}>{t.obSkip}</button><button className="onb-button onb-button--primary" type="button" onClick={() => void onPersist("gate")}>{t.onboarding.continue}</button></div>
    </section>
  );
}
