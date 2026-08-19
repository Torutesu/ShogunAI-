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
import appIconUrl from "../../src-tauri/icons/icon-128.png";
import {
  armPermissionDrag,
  disarmPermissionDrag,
  EMPTY_PERMISSIONS,
  exclusionCategories,
  getDraftStop,
  getOnboardingState,
  IN_TAURI,
  permissionStatus,
  requestAxPermission,
  requestMicrophonePermission,
  requestScreenRecordingPermission,
  setDraftStop,
  setOnboardingState,
  STEPS,
  track,
} from "./ipc";
import type { ExclusionCategory, OnboardingState, PermissionSnapshot } from "./ipc";

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
  const [permissions, setPermissions] = useState<PermissionSnapshot>(EMPTY_PERMISSIONS);
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
    void Promise.all([getOnboardingState(), permissionStatus()]).then(([s, permission]) => {
      if (!alive) return;
      setState(s);
      setPermissions(permission);
      if (!shownLogged.current) {
        shownLogged.current = true;
        track("shown");
      }
    });
    if (!IN_TAURI) return;
    const offs: Array<Promise<() => void>> = [];
    // Pushed by the native coordinator on initial status, permission edges, request completion,
    // and application activation. Bootstrap read above covers listener registration races.
    offs.push(listen<PermissionSnapshot>("permissions-changed", (e) => setPermissions(e.payload)));
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
  const isPermissionsRepair = state?.permissions_repair === true;

  // Track each step view once per arrival.
  const lastTracked = useRef<string | null>(null);
  useEffect(() => {
    if (!state) return;
    if (lastTracked.current !== step) {
      lastTracked.current = step;
      track(step);
    }
  }, [state, step]);

  // Log the grant exactly once, when it first flips on.
  useEffect(() => {
    if (permissions.all_granted && !grantedLogged.current) {
      grantedLogged.current = true;
      track("all_permissions_granted");
    }
  }, [permissions.all_granted]);

  const go = useCallback(
    (delta: number): void => {
      if (!state) return;
      const next = STEPS[Math.min(STEPS.length - 1, Math.max(0, idx + delta))];
      const record = { ...state, step: next };
      void setOnboardingState(record).then((saved) => {
        if (saved) setState(saved);
      });
    },
    [idx, state],
  );

  const finish = useCallback((): void => {
    if (!state) return;
    if (!permissions.all_granted) return;
    const record = { ...state, completed: true, step: "ready" as const, permissions_repair: false };
    // The completing write is the flow's single exit: Rust stamps the trial (once, ever) and
    // closes this window.
    void setOnboardingState(record).then((saved) => {
      if (saved) setState(saved);
    });
  }, [permissions.all_granted, state]);

  if (!state) return <div className="onb" />;

  // Per-step footer. `skip` is offered only where skipping leaves a working product — never on
  // the steps that are pure explanation, where there is nothing to skip.
  const last = idx === STEPS.length - 1;
  const primaryLabel = isPermissionsRepair
    ? t.obReadyStart
    : step === "welcome"
      ? t.obWelcomeStart
      : last
        ? t.obReadyStart
        : t.obNext;
  const canSkip = !isPermissionsRepair && (step === "plan" || step === "connect");
  const permissionsBlocked = step === "permission" && !permissions.all_granted;

  return (
    <div className="onb">
      <div className="onb-card glass ob-card">
        <header className="onb-head">
          <span className="onb-brand">
            <span className="onb-mark">⚔</span>
            {o.brand}
          </span>
          {!isPermissionsRepair ? (
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
          {step === "permission" ? <Permission liveApp={liveApp} permissions={permissions} /> : null}
          {step === "plan" ? <Plan state={state} onChange={setState} /> : null}
          {step === "connect" ? <Connect /> : null}
          {step === "ready" ? <Ready /> : null}
        </div>

        <footer className="ob-foot">
          {idx > 0 && !isPermissionsRepair ? (
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
              disabled={permissionsBlocked}
              onClick={isPermissionsRepair || last ? finish : () => go(1)}
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

type PermissionKind = "accessibility" | "microphone" | "screen";

function PermissionIcon({ kind }: { kind: PermissionKind }): JSX.Element {
  if (kind === "microphone") {
    return (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <rect x="9" y="3" width="6" height="11" rx="3" />
        <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21M9 21h6" />
      </svg>
    );
  }
  if (kind === "screen") {
    return (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="4" width="18" height="13" rx="2" />
        <path d="M8 21h8M12 17v4" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 3l7.5 3.2v5.3c0 4.7-3.2 8.4-7.5 10.2-4.3-1.8-7.5-5.5-7.5-10.2V6.2L12 3z" />
      <path d="M9 11.8l2.2 2.2L15.2 10" />
    </svg>
  );
}

function PermissionRow(props: {
  kind: PermissionKind;
  title: string;
  detail: string;
  granted: boolean;
  onAction: () => void;
}): JSX.Element {
  return (
    <div className={`ob-permission${props.granted ? " is-ready" : ""}`}>
      <span className="ob-permission-icon"><PermissionIcon kind={props.kind} /></span>
      <span className="ob-permission-copy">
        <strong>{props.title}</strong>
        <span>{props.detail}</span>
      </span>
      {props.granted ? (
        <span className="ob-permission-ready"><CheckIcon />{o.permissionReady}</span>
      ) : (
        <button className="onb-btn ob-permission-action" type="button" onClick={props.onAction}>
          {o.permissionAction}
        </button>
      )}
    </div>
  );
}

function PermissionDrag({ label, onOpen }: { label: string; onOpen: () => void }): JSX.Element {
  return (
    <div className="ob-drag-helper">
      <button
        className="ob-drag-app"
        type="button"
        aria-label={o.dragAria.replace("{permission}", label)}
        onPointerEnter={() => void armPermissionDrag()}
        onPointerLeave={(event) => {
          if (event.buttons === 0) void disarmPermissionDrag();
        }}
        onPointerDown={(event) => {
          if (event.button === 0) {
            track("permission_app_drag_started");
            void armPermissionDrag();
          }
        }}
        onClick={onOpen}
      >
        <img src={appIconUrl} alt="" draggable={false} />
        <span><strong>{o.dragTitle}</strong><small>{o.dragHint}</small></span>
        <span className="ob-drag-cue" aria-hidden="true">↗</span>
      </button>
      <span className="ob-drag-wait"><span className="onb-spin" />{o.waiting}</span>
    </div>
  );
}

/** PermissionFlow-style center: every required capability is visible together, live, and backed
 * by native macOS status/request APIs. */
function Permission({ liveApp, permissions }: { liveApp: string; permissions: PermissionSnapshot }): JSX.Element {
  const [opened, setOpened] = useState<"accessibility" | "screen" | null>(null);
  const readyCount = Number(permissions.accessibility) + Number(permissions.microphone) + Number(permissions.screen_recording);

  const openAccessibility = (): void => {
    setOpened("accessibility");
    track("accessibility_settings_opened");
    void requestAxPermission();
  };
  const requestMicrophone = (): void => {
    track("microphone_requested");
    void requestMicrophonePermission();
  };
  const requestScreen = (): void => {
    setOpened("screen");
    track("screen_recording_requested");
    void requestScreenRecordingPermission();
  };

  return (
    <section className="obs ob-permission-center">
      <div className="ob-permission-heading">
        <div>
          <h1 className="onb-title">{t.obPermTitle}</h1>
          <p className="onb-lead">{t.obPermBody}</p>
        </div>
        <span className={`onb-badge${permissions.all_granted ? " ok" : ""}`}>
          <span className="onb-dot" />
          {o.readyCount.replace("{n}", String(readyCount))}
        </span>
      </div>

      <div className="ob-permission-rail" aria-label={o.permissionsLabel}>
        <PermissionRow kind="accessibility" title={o.accessibilityTitle} detail={o.accessibilityDetail} granted={permissions.accessibility} onAction={openAccessibility} />
        {opened === "accessibility" && !permissions.accessibility ? <PermissionDrag label={o.accessibilityTitle} onOpen={openAccessibility} /> : null}
        <PermissionRow kind="microphone" title={o.microphoneTitle} detail={o.microphoneDetail} granted={permissions.microphone} onAction={requestMicrophone} />
        <PermissionRow kind="screen" title={o.screenTitle} detail={o.screenDetail} granted={permissions.screen_recording} onAction={requestScreen} />
        {opened === "screen" && !permissions.screen_recording ? <PermissionDrag label={o.screenTitle} onOpen={requestScreen} /> : null}
      </div>

      {permissions.accessibility ? (
        <div className="ob-perm-proof">{t.obPermProof.replace("{app}", liveApp || t.yourScreen)}</div>
      ) : null}
      <div className="ob-permission-note"><ShieldIcon />{o.privacyNote}</div>
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
    void setOnboardingState(next).then((saved) => {
      if (saved) onChange(saved);
    });
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
