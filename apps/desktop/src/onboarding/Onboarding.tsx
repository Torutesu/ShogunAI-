// First run (issue #6). Six steps, in the dedicated onboarding window that issue #46 proved on
// device (Accessory-app front-ordering, all-Spaces float, permission watcher — see onboarding.rs).
//
// The order is the argument: what it does → what it reads and never keeps → permission → plan →
// connections → how to reach it. Nothing is asked for before the reason to grant it has been
// given, which is why the privacy contract sits in front of the permission prompt and not after.
//
// Everything factual on screen comes from the core (see ./ipc.ts): whether Accessibility is
// granted, what is never read, which services exist, whether drafts-only is on. Nothing here
// asserts a fact of its own — and progress itself is persisted by Rust, so a quit mid-flow
// resumes rather than restarts (invariant 1).

import { useCallback, useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { count, t } from "../strings";
import { ConnectionsList } from "../connections";
import { AnalyticsToggle } from "../AnalyticsToggle";
import { comboChips, DEFAULT_BINDS } from "../keys";
import {
  axPermission,
  exclusionCategories,
  getDraftStop,
  getOnboardingState,
  IN_TAURI,
  requestAxPermission,
  setDraftStop,
  setOnboardingState,
  STEPS,
  track,
} from "./ipc";
import type { ExclusionCategory, OnboardingState } from "./ipc";

type Appearance = "auto" | "light" | "dark";

// The #46 permission copy block — reused verbatim as the heart of the permission step.
const o = t.onboarding;

function appName(bundle: string): string {
  if (!bundle) return t.yourScreen;
  const seg = bundle.split(".").pop() || bundle;
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}

function CheckIcon(): JSX.Element {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--live)" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 12l5 5L20 6" />
    </svg>
  );
}

function ShieldIcon(): JSX.Element {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--muted)" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 3l7.5 3.2v5.3c0 4.7-3.2 8.4-7.5 10.2-4.3-1.8-7.5-5.5-7.5-10.2V6.2L12 3z" />
    </svg>
  );
}

function MoonIcon(): JSX.Element {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
    </svg>
  );
}

export function Onboarding(): JSX.Element {
  // null = the persisted state hasn't come back yet — render nothing rather than flash step 1 at
  // someone who was halfway through.
  const [state, setState] = useState<OnboardingState | null>(null);
  const [granted, setGranted] = useState(false);
  // The app SHOGUN is reading right now — the proof shown the moment permission lands. The
  // "context" event is broadcast by the capture engine to every webview of the app.
  const [liveApp, setLiveApp] = useState("");
  const shownLogged = useRef(false);
  const grantedLogged = useRef(false);

  // Theme: shared with the notch window through same-origin localStorage. The value is stored
  // JSON-encoded (App.tsx saveJson), so it must be parsed — reading it raw yields `"dark"` with
  // quotes, which matches no CSS selector and silently pinned this window to dark.
  useEffect(() => {
    let appearance: Appearance = "auto";
    try {
      const raw = localStorage.getItem("shogun.appearance");
      if (raw) appearance = JSON.parse(raw) as Appearance;
    } catch {
      /* fall back to auto */
    }
    document.documentElement.setAttribute("data-appearance", appearance);
  }, []);

  useEffect(() => {
    let alive = true;
    // Hydrate progress and live trust together. The Rust watcher can emit its initial event before
    // WebKit finishes registering listeners, so the bootstrap read must independently establish
    // the correct permission state.
    void Promise.all([getOnboardingState(), axPermission()]).then(([s, permission]) => {
      if (!alive) return;
      setState(s);
      setGranted(permission);
      if (!shownLogged.current) {
        shownLogged.current = true;
        track("shown");
      }
    });
    if (!IN_TAURI) return;
    const offs: Array<Promise<() => void>> = [];
    // Pushed by the Rust watcher on every trust edge while this window is open; the poll below
    // backs it up in case a push is missed.
    offs.push(listen<boolean>("accessibility-changed", (e) => setGranted(e.payload)));
    offs.push(
      listen<{ bundle_id: string; title_masked: string }>("context", (e) =>
        setLiveApp(appName(e.payload.bundle_id)),
      ),
    );
    return () => {
      alive = false;
      offs.forEach((p) => void p.then((off) => off()));
    };
  }, []);

  const idx = Math.max(0, state ? STEPS.indexOf(state.step) : 0);
  const step = state ? STEPS[idx] : "welcome";
  const isAccessibilityRepair = state?.accessibility_repair === true;

  // Track each step view once per arrival.
  const lastTracked = useRef<string | null>(null);
  useEffect(() => {
    if (!state) return;
    if (lastTracked.current !== step) {
      lastTracked.current = step;
      track(step);
    }
  }, [state, step]);

  // Accessibility is polled HERE, not inside the step, because the footer depends on it: once the
  // permission is granted there is nothing left to skip, and offering "Skip for now" next to a
  // green "Granted" reads as a way to undo it. The check is the NON-prompting one (see ipc.ts) —
  // polling a prompting check would reopen the system dialog every second and a half.
  useEffect(() => {
    if (step !== "permission") return;
    let alive = true;
    let checking = false;
    const tick = (): void => {
      if (checking) return;
      checking = true;
      void axPermission()
        .then((ok) => {
          if (alive) setGranted(ok);
        })
        .finally(() => {
          checking = false;
        });
    };
    const checkWhenVisible = (): void => {
      if (document.visibilityState === "visible") tick();
    };
    tick();
    const id = setInterval(tick, 1500);
    window.addEventListener("focus", tick);
    document.addEventListener("visibilitychange", checkWhenVisible);
    return () => {
      alive = false;
      clearInterval(id);
      window.removeEventListener("focus", tick);
      document.removeEventListener("visibilitychange", checkWhenVisible);
    };
  }, [step]);

  // Log the grant exactly once, when it first flips on.
  useEffect(() => {
    if (granted && !grantedLogged.current) {
      grantedLogged.current = true;
      track("ax_granted");
    }
  }, [granted]);

  const go = useCallback(
    (delta: number): void => {
      if (!state) return;
      const next = STEPS[Math.min(STEPS.length - 1, Math.max(0, idx + delta))];
      const record = { ...state, step: next };
      setState(record);
      void setOnboardingState(record);
    },
    [idx, state],
  );

  const finish = useCallback((): void => {
    if (!state) return;
    const record = { ...state, completed: true, step: "ready" as const, accessibility_repair: false };
    setState(record);
    // The completing write is the flow's single exit: Rust stamps the trial (once, ever) and
    // closes this window.
    void setOnboardingState(record);
  }, [state]);

  if (!state) return <div className="onb" />;

  // Per-step footer. `skip` is offered only where skipping leaves a working product — never on
  // the steps that are pure explanation, where there is nothing to skip.
  const last = idx === STEPS.length - 1;
  const primaryLabel = isAccessibilityRepair
    ? t.obReadyStart
    : step === "welcome"
      ? t.obWelcomeStart
      : last
        ? t.obReadyStart
        : t.obNext;
  const canSkip = !isAccessibilityRepair && ((step === "permission" && !granted) || step === "plan" || step === "connect");

  return (
    <div className="onb">
      <div className="onb-card glass ob-card">
        <header className="onb-head">
          <span className="onb-brand">
            <span className="onb-mark">⚔</span>
            {o.brand}
          </span>
          {!isAccessibilityRepair ? (
            <>
              <div className="ob-prog" role="presentation">
                {STEPS.map((s, i) => (
                  <span key={s} className={`ob-seg${i <= idx ? " is-done" : ""}`} />
                ))}
              </div>
              <span className="ob-count">
                {t.obStep.replace("{n}", String(idx + 1)).replace("{total}", String(STEPS.length))}
              </span>
            </>
          ) : null}
        </header>

        <div className="onb-body ob-body" key={step}>
          {step === "welcome" ? <Welcome /> : null}
          {step === "reads" ? <Reads /> : null}
          {step === "permission" ? <Permission liveApp={liveApp} granted={granted} /> : null}
          {step === "plan" ? <Plan state={state} onChange={setState} /> : null}
          {step === "connect" ? <Connect /> : null}
          {step === "ready" ? <Ready /> : null}
        </div>

        <footer className="ob-foot">
          {idx > 0 && !isAccessibilityRepair ? (
            <button className="onb-btn ghost" type="button" onClick={() => go(-1)}>
              {t.obBack}
            </button>
          ) : (
            <span />
          )}
          <div className="ob-acts">
            {canSkip ? (
              <button className="onb-btn ghost" type="button" onClick={() => go(1)}>
                {t.obSkip}
              </button>
            ) : null}
            <button
              className="onb-btn primary"
              type="button"
              disabled={isAccessibilityRepair && !granted}
              onClick={isAccessibilityRepair || last ? finish : () => go(1)}
            >
              {primaryLabel}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}

/* ── steps ──────────────────────────────────────────────────────────────── */

function Welcome(): JSX.Element {
  return (
    <section className="obs">
      <h1 className="onb-title">{t.obWelcomeTitle}</h1>
      <p className="onb-lead">{t.obWelcomeBody}</p>
      <ul className="ob-points">
        {[t.obWelcomePoint1, t.obWelcomePoint2].map((p) => (
          <li key={p}>
            <CheckIcon />
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
      <h1 className="onb-title">{t.obReadsTitle}</h1>
      <p className="onb-lead">{t.obReadsBody}</p>
      <div className="ob-facts">
        {facts.map((f) => (
          <div key={f.t} className="ob-fact">
            <div className="ob-fact-t">{f.t}</div>
            <div className="ob-fact-b">{f.b}</div>
          </div>
        ))}
      </div>
      {/* The exclusions are the strongest thing SHOGUN can say about itself, and they are a fact
          of the build, not a setting — so they are shown, never offered. */}
      <div className="ob-never">
        <div className="ob-never-head">
          <ShieldIcon />
          <span className="ob-never-title">{t.obNeverTitle}</span>
        </div>
        <div className="ob-never-list">
          {cats.map((c) => (
            <span key={c.id} className="ob-never-item">
              {t.obExclusion[c.id] ?? c.id}
              <b>{count(c.id === "sensitive_titles" ? t.obNeverRules : t.obNeverApps, c.count)}</b>
            </span>
          ))}
        </div>
        <div className="ob-never-note">{t.obNeverBody}</div>
      </div>
    </section>
  );
}

/** The permission step: title/lede from the flow, with the proven #46 guide inside it — the
 *  do/wont columns, the numbered System Settings steps, and the troubleshooting notes that appear
 *  once the user has actually opened Settings. */
function Permission({ liveApp, granted }: { liveApp: string; granted: boolean }): JSX.Element {
  const [opened, setOpened] = useState(false);

  if (granted) {
    return (
      <section className="obs">
        <h1 className="onb-title">{t.obPermTitle}</h1>
        <div className="ob-perm is-on">
          <div className="ob-perm-state">
            <CheckIcon />
            {t.obPermGranted}
          </div>
          {/* Proof, not a claim: the app it is reading, right now. */}
          <div className="ob-perm-proof">{t.obPermProof.replace("{app}", liveApp || t.yourScreen)}</div>
        </div>
      </section>
    );
  }

  return (
    <section className="obs">
      <h1 className="onb-title">{t.obPermTitle}</h1>
      <p className="onb-lead">{t.obPermBody}</p>

      <div className="onb-cols">
        <div className="onb-col">
          <span className="onb-col-h">{o.doTitle}</span>
          {o.doItems.map((it) => (
            <span className="onb-item" key={it}>
              <CheckIcon />
              {it}
            </span>
          ))}
        </div>
        <div className="onb-col">
          <span className="onb-col-h">{o.wontTitle}</span>
          {o.wontItems.map((it) => (
            <span className="onb-item" key={it}>
              <ShieldIcon />
              {it}
            </span>
          ))}
        </div>
      </div>

      <div className="onb-steps">
        <span className="onb-col-h">{o.stepsTitle}</span>
        <ol>
          {o.steps.map((s, i) => (
            <li key={s}>
              <span className="onb-num">{i + 1}</span>
              {s}
            </li>
          ))}
        </ol>
      </div>

      <div className="ob-perm-cta">
        <button
          className="onb-btn primary"
          type="button"
          onClick={() => {
            setOpened(true);
            track("ax_settings_opened");
            void requestAxPermission();
          }}
        >
          {opened ? o.ctaAgain : o.cta}
        </button>
      </div>

      {opened ? (
        <div className="onb-trouble">
          <span className="onb-waiting">
            <span className="onb-spin" />
            {o.waiting}
          </span>
          <span className="onb-col-h">{o.troubleTitle}</span>
          <ul>
            {o.troubleItems.map((it) => (
              <li key={it}>{it}</li>
            ))}
          </ul>
        </div>
      ) : (
        <div className="ob-fact">
          <div className="ob-fact-t">{t.obPermSkipTitle}</div>
          <div className="ob-fact-b">{t.obPermSkipBody}</div>
        </div>
      )}
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
    // v1 BYOK is Anthropic-first (CLAUDE.md); the provider picker lives in Settings. The key goes
    // to the Keychain and nowhere else (invariant 7).
    void invoke("set_byok_key", { provider: "anthropic", key: k })
      .then(() => {
        setSaved(true);
        setKey("");
      })
      .catch(() => undefined);
  };

  return (
    <section className="obs">
      <h1 className="onb-title">{t.obPlanTitle}</h1>
      <p className="onb-lead">{t.obPlanBody}</p>

      <div className="ob-plans">
        {(
          [
            { id: "standard", name: t.obPlanStandard, body: t.obPlanStandardBody },
            { id: "pro", name: t.obPlanPro, body: t.obPlanProBody },
          ] as const
        ).map((p) => (
          <button
            key={p.id}
            type="button"
            className={`ob-plan${plan === p.id ? " is-on" : ""}`}
            aria-pressed={plan === p.id}
            onClick={() => pick(p.id)}
          >
            <span className="ob-plan-name">{p.name}</span>
            <span className="ob-plan-body">{p.body}</span>
          </button>
        ))}
      </div>

      <div className="ob-fact">
        <div className="ob-fact-t">{t.obPlanKeys}</div>
        <div className="ob-fact-b">{t.obPlanKeysBody}</div>
      </div>

      {/* The key is asked for only by the plan that needs one, and only after the split above has
          explained why it is needed at all. */}
      {plan === "pro" ? (
        <div className="ob-fact">
          <div className="ob-fact-t">{t.obKeyTitle}</div>
          <div className="ob-fact-b">{saved ? t.keySaved : t.obKeyBody}</div>
          {!saved ? (
            <div className="ob-keyrow">
              <input
                className="ob-keyinput"
                type="password"
                autoComplete="off"
                placeholder={t.keyPlaceholders.anthropic}
                value={key}
                onChange={(e) => setKey(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveKey();
                }}
              />
              <button className="onb-btn" type="button" disabled={!key.trim()} onClick={saveKey}>
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
  // Shown when an attempt to turn drafts-only OFF is rejected (consent lives in Settings).
  const [locked, setLocked] = useState(false);
  useEffect(() => {
    void getDraftStop().then(setStop);
  }, []);

  return (
    <section className="obs">
      <h1 className="onb-title">{t.obConnectTitle}</h1>
      <p className="onb-lead">{t.obConnectBody}</p>

      <div className="ob-fact">
        <label className="ob-switchrow">
          <input
            type="checkbox"
            checked={draftStop}
            onChange={(e) => {
              const want = e.target.checked;
              setStop(want);
              setLocked(false);
              // Rust is the authority: turning draft-stop OFF without consent is rejected and the
              // toggle falls back to ON (invariant 4 — fail to the safe side).
              void setDraftStop(want).then((actual) => {
                setStop(actual);
                if (actual && !want) setLocked(true);
              });
            }}
          />
          <span>
            <span className="ob-fact-t">{t.obDraftStop}</span>
            <span className="ob-fact-b">{t.obDraftStopBody}</span>
            {locked ? <span className="ob-fact-b ob-locked">{t.obDraftStopLocked}</span> : null}
          </span>
        </label>
      </div>

      <ConnectionsList connectableOnly />
      <div className="ob-never-note">{t.obConnectSkip}</div>
    </section>
  );
}

function Ready(): JSX.Element {
  // The summon shortcut is read from the live bindings, never printed as a literal: a closing
  // step that promises ⌃⌥N to someone who rebound it in Settings is worse than no closing step.
  const [binds, setBinds] = useState<Record<string, string>>(DEFAULT_BINDS);
  useEffect(() => {
    if (!IN_TAURI) return;
    void invoke<Record<string, string>>("get_shortcuts")
      .then((b) => setBinds((cur) => ({ ...cur, ...b })))
      .catch(() => undefined);
  }, []);

  return (
    <section className="obs">
      <h1 className="onb-title">{t.obReadyTitle}</h1>
      <p className="onb-lead">{t.obReadyBody}</p>

      <div className="ob-keys">
        <span className="ob-keys-name">{t.obReadyShortcut}</span>
        <span className="ob-keys-chips">
          {comboChips(binds.summon ?? DEFAULT_BINDS.summon).map((c, i) => (
            <kbd key={`${c}${i}`}>{c}</kbd>
          ))}
        </span>
      </div>
      <div className="ob-keys">
        <span className="ob-keys-name">{t.obReadyDraft}</span>
        <span className="ob-keys-chips">
          <kbd>{t.obReadyDraftKey}</kbd>
        </span>
      </div>

      <div className="ob-fact">
        <div className="ob-fact-head">
          <MoonIcon />
          <div className="ob-fact-t">{t.obReadyTonight}</div>
        </div>
        <div className="ob-fact-b">{t.obReadyTonightBody}</div>
      </div>

      <div className="ob-fact">
        <AnalyticsToggle />
      </div>
    </section>
  );
}
