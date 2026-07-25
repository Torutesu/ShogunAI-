// First run. Six steps, in the panel itself.
//
// It runs HERE, hanging from the notch, rather than in a centred window, because the first thing
// someone has to learn about this product is where it lives. Every step after the first is
// therefore also a rehearsal of reaching for it.
//
// The order is the argument: what it does → what it reads and never keeps → permission → plan →
// connections → how to reach it. Nothing is asked for before the reason to grant it has been
// given, which is why the privacy contract sits in front of the permission prompt and not after.
//
// Everything factual on screen comes from the core (see ./ipc.ts): whether Accessibility is
// granted, what is never read, which services exist. Nothing here asserts a fact of its own.

import { useCallback, useEffect, useState } from "react";
import type { JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IN_TAURI } from "../tauri";
import { Icon } from "../icons";
import { count, t } from "../strings";
import { ConnectionsList } from "../connections";
import { TriggerChips } from "../ShortcutRows";
import { DEFAULT_TRIGGERS, parseTrigger } from "../shortcuts";
import {
  axPermission,
  exclusionCategories,
  getDraftStop,
  requestAxPermission,
  setDraftStop,
  setOnboardingState,
  STEPS,
} from "./ipc";
import type { ExclusionCategory, OnboardingState } from "./ipc";

export function Onboarding(props: {
  state: OnboardingState;
  /** The app SHOGUN is reading right now — the proof that permission worked. */
  liveApp: string;
  onChange: (next: OnboardingState) => void;
  onDone: () => void;
}): JSX.Element {
  const { state, liveApp, onChange, onDone } = props;
  const idx = Math.max(0, STEPS.indexOf(state.step));
  const step = STEPS[idx];

  const go = useCallback(
    (delta: number): void => {
      const next = STEPS[Math.min(STEPS.length - 1, Math.max(0, idx + delta))];
      const record = { ...state, step: next };
      onChange(record);
      void setOnboardingState(record);
    },
    [idx, state, onChange],
  );

  const finish = useCallback((): void => {
    const record = { ...state, completed: true };
    onChange(record);
    void setOnboardingState(record);
    onDone();
  }, [state, onChange, onDone]);

  // Accessibility is polled HERE, not inside the step, because the footer depends on it: once the
  // permission is granted there is nothing left to skip, and offering "Skip for now" next to a
  // green "Granted" reads as a way to undo it. The check is the NON-prompting one (see ipc.ts) —
  // polling a prompting check would reopen the system dialog every second and a half.
  const [axGranted, setAxGranted] = useState(false);
  useEffect(() => {
    if (step !== "permission" || axGranted) return;
    let alive = true;
    const tick = (): void =>
      void axPermission().then((ok) => {
        if (alive) setAxGranted(ok);
      });
    tick();
    const id = setInterval(tick, 1500);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [step, axGranted]);

  // Per-step footer. `skip` is offered only where skipping leaves a working product — never on
  // the steps that are pure explanation, where there is nothing to skip.
  const last = idx === STEPS.length - 1;
  const primaryLabel =
    step === "welcome" ? t.obWelcomeStart : last ? t.obReadyStart : t.obNext;
  const canSkip =
    (step === "permission" && !axGranted) || step === "plan" || step === "connect";

  return (
    <div className="ob">
      <header className="ob__head">
        <span className="ob__mark" aria-hidden="true">
          ⚔
        </span>
        <div className="ob__prog" role="presentation">
          {STEPS.map((s, i) => (
            <span key={s} className={`ob__seg${i <= idx ? " is-done" : ""}`} />
          ))}
        </div>
        <span className="ob__count">
          {t.obStep.replace("{n}", String(idx + 1)).replace("{total}", String(STEPS.length))}
        </span>
      </header>

      <div className="ob__body" key={step}>
        {step === "welcome" ? <Welcome /> : null}
        {step === "reads" ? <Reads /> : null}
        {step === "permission" ? <Permission liveApp={liveApp} granted={axGranted} /> : null}
        {step === "plan" ? <Plan state={state} onChange={onChange} /> : null}
        {step === "connect" ? <Connect /> : null}
        {step === "ready" ? <Ready /> : null}
      </div>

      <footer className="ob__foot">
        {idx > 0 ? (
          <button className="ob__ghost" type="button" onClick={() => go(-1)}>
            <Icon name="back" size={15} />
            {t.obBack}
          </button>
        ) : (
          <span />
        )}
        <div className="ob__acts">
          {canSkip ? (
            <button className="ob__ghost" type="button" onClick={() => go(1)}>
              {t.obSkip}
            </button>
          ) : null}
          <button className="ob__go" type="button" onClick={last ? finish : () => go(1)}>
            {primaryLabel}
          </button>
        </div>
      </footer>
    </div>
  );
}

/* ── steps ──────────────────────────────────────────────────────────────── */

function Welcome(): JSX.Element {
  return (
    <section className="obs">
      <h1 className="obs__title">{t.obWelcomeTitle}</h1>
      <p className="obs__lede">{t.obWelcomeBody}</p>
      <ul className="obs__points">
        {[t.obWelcomePoint1, t.obWelcomePoint2].map((p) => (
          <li key={p}>
            <Icon name="check" size={15} />
            {p}
          </li>
        ))}
      </ul>
    </section>
  );
}

function Reads(): JSX.Element {
  const [cats, setCats] = useState<ExclusionCategory[]>([]);
  useEffect(() => {
    void exclusionCategories().then(setCats);
  }, []);

  const facts = [
    { t: t.obReadsKeep1Title, b: t.obReadsKeep1Body },
    { t: t.obReadsKeep2Title, b: t.obReadsKeep2Body },
    { t: t.obReadsKeep3Title, b: t.obReadsKeep3Body },
  ];

  return (
    <section className="obs">
      <h1 className="obs__title">{t.obReadsTitle}</h1>
      <p className="obs__lede">{t.obReadsBody}</p>
      <div className="obs__facts">
        {facts.map((f) => (
          <div key={f.t} className="fact">
            <div className="fact__t">{f.t}</div>
            <div className="fact__b">{f.b}</div>
          </div>
        ))}
      </div>
      {/* The exclusions are the strongest thing SHOGUN can say about itself, and they are a fact
          of the build, not a setting — so they are shown, never offered. */}
      <div className="never">
        <div className="never__head">
          <Icon name="shield" size={16} />
          <span className="never__title">{t.obNeverTitle}</span>
        </div>
        <div className="never__list">
          {cats.map((c) => (
            <span key={c.id} className="never__item">
              {t.obExclusion[c.id] ?? c.id}
              <b>{count(c.id === "sensitive_titles" ? t.obNeverRules : t.obNeverApps, c.count)}</b>
            </span>
          ))}
        </div>
        <div className="never__note">{t.obNeverBody}</div>
      </div>
    </section>
  );
}

function Permission({ liveApp, granted }: { liveApp: string; granted: boolean }): JSX.Element {
  const [asked, setAsked] = useState(false);

  return (
    <section className="obs">
      <h1 className="obs__title">{t.obPermTitle}</h1>
      <p className="obs__lede">{t.obPermBody}</p>

      <div className={`perm${granted ? " is-on" : ""}`}>
        {granted ? (
          <>
            <div className="perm__state">
              <span className="live__dot" />
              {t.obPermGranted}
            </div>
            {/* Proof, not a claim: the app it is reading, right now. */}
            <div className="perm__proof">{t.obPermProof.replace("{app}", liveApp)}</div>
          </>
        ) : (
          <>
            <button
              className="ob__go"
              type="button"
              onClick={() => {
                setAsked(true);
                void requestAxPermission();
              }}
            >
              {t.obPermGrant}
            </button>
            {asked ? (
              <div className="perm__wait">
                <span className="think__dot" />
                <span className="think__dot" />
                <span className="think__dot" />
                {t.obPermWaiting}
              </div>
            ) : null}
          </>
        )}
      </div>

      {!granted ? (
        <div className="obs__aside">
          <div className="fact__t">{t.obPermSkipTitle}</div>
          <div className="fact__b">{t.obPermSkipBody}</div>
        </div>
      ) : null}
    </section>
  );
}

function Plan(props: { state: OnboardingState; onChange: (s: OnboardingState) => void }): JSX.Element {
  const { state, onChange } = props;
  const [key, setKey] = useState("");
  const [saved, setSaved] = useState(false);
  const plan = state.plan;

  const pick = (p: "standard" | "pro"): void => {
    const next = { ...state, plan: p };
    onChange(next);
    void setOnboardingState(next);
  };

  const saveKey = (): void => {
    const k = key.trim();
    if (!k) return;
    if (!IN_TAURI) {
      setSaved(true);
      setKey("");
      return;
    }
    // v1 BYOK is Anthropic only (CLAUDE.md); the provider picker lives in Settings.
    void invoke("set_byok_key", { provider: "anthropic", key: k })
      .then(() => {
        setSaved(true);
        setKey("");
      })
      .catch(() => undefined);
  };

  return (
    <section className="obs">
      <h1 className="obs__title">{t.obPlanTitle}</h1>
      <p className="obs__lede">{t.obPlanBody}</p>

      <div className="plans">
        {(
          [
            { id: "standard", name: t.obPlanStandard, body: t.obPlanStandardBody },
            { id: "pro", name: t.obPlanPro, body: t.obPlanProBody },
          ] as const
        ).map((p) => (
          <button
            key={p.id}
            type="button"
            className={`plan${plan === p.id ? " is-on" : ""}`}
            aria-pressed={plan === p.id}
            onClick={() => pick(p.id)}
          >
            <span className="plan__tick">
              <Icon name="check" size={13} />
            </span>
            <span className="plan__name">{p.name}</span>
            <span className="plan__body">{p.body}</span>
          </button>
        ))}
      </div>

      <div className="fact">
        <div className="fact__t">{t.obPlanKeys}</div>
        <div className="fact__b">{t.obPlanKeysBody}</div>
      </div>

      {/* The key is asked for only by the plan that needs one, and only after the split above has
          explained why it is needed at all. */}
      {plan === "pro" ? (
        <div className="obs__aside">
          <div className="fact__t">{t.obKeyTitle}</div>
          <div className="fact__b">{saved ? t.keySaved : t.obKeyBody}</div>
          {!saved ? (
            <div className="keyrow">
              <input
                className="keyrow__input"
                type="password"
                autoComplete="off"
                placeholder={t.keyPlaceholders.anthropic}
                value={key}
                onFocus={() => {
                  if (IN_TAURI) void invoke("focus_field", { focused: true }).catch(() => undefined);
                }}
                onChange={(e) => setKey(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveKey();
                }}
              />
              <button
                className="keyrow__btn keyrow__btn--go"
                type="button"
                disabled={!key.trim()}
                onClick={saveKey}
              >
                {t.keySave}
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function Connect(): JSX.Element {
  const [draftStop, setStop] = useState(true);
  useEffect(() => {
    void getDraftStop().then(setStop);
  }, []);

  return (
    <section className="obs">
      <h1 className="obs__title">{t.obConnectTitle}</h1>
      <p className="obs__lede">{t.obConnectBody}</p>

      <div className="swrow">
        <div>
          <div className="set__label" id="ob-draft-stop">
            {t.obDraftStop}
          </div>
          <div className="set__hint">{t.obDraftStopBody}</div>
        </div>
        <button
          type="button"
          role="switch"
          className="sw"
          aria-checked={draftStop}
          aria-labelledby="ob-draft-stop"
          onClick={() => {
            const next = !draftStop;
            setStop(next);
            void setDraftStop(next);
          }}
        />
      </div>

      <ConnectionsList connectableOnly />
      <div className="obs__note">{t.obConnectSkip}</div>
    </section>
  );
}

function Ready(): JSX.Element {
  // The two triggers taught here are read from the live bindings, never printed as literals: a
  // closing step that promises ⌃⌥ to someone who rebound it in the previous minute is worse than
  // no closing step.
  const [binds, setBinds] = useState<Record<string, string>>(DEFAULT_TRIGGERS);
  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<Record<string, string>>("get_shortcuts")
      .then((b) => setBinds((cur) => ({ ...cur, ...b })))
      .catch(() => undefined);
  }, []);

  const rows: Array<[string, string]> = [
    [t.obReadyShortcut, binds.summon ?? DEFAULT_TRIGGERS.summon],
    [t.obReadyDraft, binds.draft ?? DEFAULT_TRIGGERS.draft],
  ];

  return (
    <section className="obs">
      <h1 className="obs__title">{t.obReadyTitle}</h1>
      <p className="obs__lede">{t.obReadyBody}</p>

      {rows.map(([label, combo]) => {
        const trigger = parseTrigger(combo);
        return (
          <div key={label} className="keys">
            <span className="keys__name">{label}</span>
            {trigger ? <TriggerChips trigger={trigger} /> : null}
          </div>
        );
      })}

      <div className="fact fact--wide">
        <div className="fact__head">
          <Icon name="moon" size={16} />
          <div className="fact__t">{t.obReadyTonight}</div>
        </div>
        <div className="fact__b">{t.obReadyTonightBody}</div>
      </div>
    </section>
  );
}
