// The onboarding flow's contract with the Rust core.
//
// Every question onboarding asks about the machine — is Accessibility granted, what is never
// read, is a key present, is the drafts-only mode on — is answered by Rust, not decided here
// (invariant 1). This module is the single list of what the core provides, so the flow itself
// stays presentation-only.
//
// Command mapping (kept aligned with src-tauri):
// - onboarding_state / set_onboarding_state ........ onboarding.rs (Rust-owned progress + trial)
// - accessibility_status ........................... onboarding.rs (NON-prompting AX check — the
//   1.5s poll must never reopen the system dialog)
// - open_accessibility_settings .................... onboarding.rs (one-shot prompt + deep link)
// - exclusion_categories ........................... exclusions.rs (live policy, never hardcoded)
// - composio_settings / set_composio_policy ........ approvals.rs — draft-stop's single source of
//   truth is ComposioPolicy (composio.json). Onboarding does NOT keep its own copy; turning it
//   off without consent is rejected by Rust and the toggle falls back to ON (invariant 4).
// - onboarding_event ............................... onboarding.rs → #91 PostHog adapter, behind
//   its opt_out gate; names are allowlisted on the Rust side.

import { invoke } from "@tauri-apps/api/core";

export const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

/** Invoke that degrades to a safe, honest fallback outside Tauri (`pnpm dev:vite` in a browser)
 *  or when a command fails — the flow then reads as "not granted / nothing connected", never as
 *  a fabricated success. */
function ask<T>(cmd: string, args: Record<string, unknown>, fallback: T): Promise<T> {
  if (!IN_TAURI) return Promise.resolve(fallback);
  return invoke<T>(cmd, args).catch(() => fallback);
}

/** Where the user got to. Persisted by Rust so a quit mid-flow resumes, not restarts. */
export type StepId = "welcome" | "reads" | "permission" | "plan" | "connect" | "ready";

export const STEPS: StepId[] = ["welcome", "reads", "permission", "plan", "connect", "ready"];

export interface OnboardingState {
  /** True once the user has finished (or explicitly skipped to the end). */
  completed: boolean;
  /** The furthest step reached, so quitting mid-flow resumes there. */
  step: StepId;
  /** Which plan the user said they wanted. Billing is a separate flow; this only decides whether
   *  onboarding asks for a key. Real entitlement gating is a Rust-core follow-up. */
  plan: "standard" | "pro" | null;
  /** Present only when macOS trust was lost after setup. The UI then shows only the repair card. */
  accessibility_repair?: boolean;
}

/** A category of thing SHOGUN never reads. Rust owns the list — the UI must not hardcode it,
 *  because a UI that lists yesterday's exclusions is a lie about today's behaviour. */
export interface ExclusionCategory {
  /** Stable id, looked up in the string catalogue for its label. */
  id: string;
  /** How many bundle ids / rules the category covers, for "6 apps" style copy. */
  count: number;
}

const FIRST_RUN: OnboardingState = { completed: false, step: "welcome", plan: null };

/** `onboarding_state() -> OnboardingState`
 *
 * A build whose core does not answer this reports COMPLETED, not first-run. The flow's exit is
 * `set_onboarding_state`, so if the core cannot remember that the user finished, showing the flow
 * would trap them in it on every launch. (In a plain browser tab the flow renders because the
 * window only exists when Rust decided to show it — outside Tauri this module is only exercised
 * by `pnpm dev:vite` for review.) */
export function getOnboardingState(): Promise<OnboardingState> {
  if (!IN_TAURI) return Promise.resolve({ ...FIRST_RUN });
  return invoke<OnboardingState>("onboarding_state")
    .then((s) => ({ ...FIRST_RUN, ...s }))
    .catch(() => ({ ...FIRST_RUN, completed: true }));
}

/** `set_onboarding_state(step, plan, completed)` — a whole-record write; the flow has one writer,
 *  and a partial update would let a resumed session disagree with itself. Rust stamps the trial on
 *  the first completing write and closes the window. */
export function setOnboardingState(next: OnboardingState): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("set_onboarding_state", {
    step: next.step,
    plan: next.plan,
    completed: next.completed,
  }).catch(() => undefined);
}

/** `accessibility_status() -> bool` — whether this process is trusted for Accessibility. The
 *  NON-prompting check: polling a prompting one would reopen the system dialog every 1.5s. */
export function axPermission(): Promise<boolean> {
  return ask<boolean>("accessibility_status", {}, false);
}

/** `open_accessibility_settings()` — the prompting variant, fired once from the button; also
 *  opens System Settings at the right pane for when the prompt has already been answered. */
export function requestAxPermission(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("open_accessibility_settings").catch(() => undefined);
}

/** `exclusion_categories() -> ExclusionCategory[]` — read from the live ExclusionPolicy so
 *  onboarding shows what is actually enforced. Fallback mirrors the built-in defaults for the
 *  browser preview only; on device an empty answer stays empty (honest). */
export function exclusionCategories(): Promise<ExclusionCategory[]> {
  return ask<ExclusionCategory[]>("exclusion_categories", {}, [
    { id: "password_managers", count: 6 },
    { id: "auth_dialog", count: 1 },
    { id: "terminals", count: 5 },
    { id: "private_browsing", count: 4 },
  ]);
}

/** Draft-stop (drafts-only mode, invariant 4). Single source of truth: ComposioPolicy
 *  (`composio.json`), the same record the L3 send gate consults. Default ON; any failure to read
 *  reports ON (fail-safe). */
export function getDraftStop(): Promise<boolean> {
  if (!IN_TAURI) return Promise.resolve(true);
  return invoke<{ draft_stop: boolean; consent_acknowledged: boolean }>("composio_settings")
    .then((s) => s.draft_stop)
    .catch(() => true);
}

/** Attempt to set draft-stop, preserving the current consent flag. Rust rejects draft_stop=false
 *  without consent (FR-C2-02/03) — the caller must treat a rejection as "still ON". Resolves to
 *  the value that is actually in force after the attempt. */
export function setDraftStop(enabled: boolean): Promise<boolean> {
  if (!IN_TAURI) return Promise.resolve(enabled);
  return invoke<{ draft_stop: boolean; consent_acknowledged: boolean }>("composio_settings")
    .then((s) =>
      invoke<void>("set_composio_policy", {
        draftStop: enabled,
        consentAcknowledged: s.consent_acknowledged,
      }).then(
        () => enabled,
        () => true, // rejected (no consent) → draft-stop stays ON, the safe side
      ),
    )
    .catch(() => true);
}

/** Fire-and-forget a funnel event. Rust allowlists the name and routes it through the #91
 *  analytics adapter (opt-out respected); nothing but the step id ever travels. */
export function track(name: string): void {
  if (IN_TAURI) void invoke("onboarding_event", { name }).catch(() => undefined);
}
