// The onboarding flow's contract with the Rust core.
//
// Every question onboarding asks about the machine — is Accessibility granted, what is never
// read, is a key present, is the drafts-only mode on — is answered by Rust, not decided here
// (invariant 1). This module is the single list of what the core must provide, so the Swift-side
// work is a checklist rather than an archaeology exercise, and so the browser preview has exactly
// one surface to fake.
//
// STATUS: the commands below marked (new) are NOT implemented in src-tauri yet. Each call falls
// back to a safe, honest value when the command is missing, so the flow runs — and reads as
// "not granted / nothing connected / no key" rather than pretending. See
// docs/onboarding-design.md for the Rust-side work.

import { invoke } from "@tauri-apps/api/core";
import { ask, IN_TAURI } from "../tauri";

/** Where the user got to. Persisted by Rust so a quit mid-flow resumes, not restarts. */
export type StepId = "welcome" | "reads" | "permission" | "plan" | "connect" | "ready";

export const STEPS: StepId[] = ["welcome", "reads", "permission", "plan", "connect", "ready"];

export interface OnboardingState {
  /** True once the user has finished (or explicitly skipped to the end). */
  completed: boolean;
  /** The furthest step reached, so quitting mid-flow resumes there. */
  step: StepId;
  /** Which plan the user said they wanted. Billing is a separate flow; this only decides whether
   *  onboarding asks for a key. */
  plan: "standard" | "pro" | null;
}

/** A category of thing SHOGUN never reads. Rust owns the list — the UI must not hardcode it,
 *  because a UI that lists yesterday's exclusions is a lie about today's behaviour. */
export interface ExclusionCategory {
  /** Stable id, looked up in the string catalogue for its label. */
  id: string;
  /** How many bundle ids / rules the category covers, for "and 6 more" style copy. */
  count: number;
}

const FIRST_RUN: OnboardingState = { completed: false, step: "welcome", plan: null };

/** (new) `onboarding_state() -> OnboardingState`
 *
 * A build whose core does not answer this yet reports COMPLETED, not first-run. The flow's exit
 * is `set_onboarding_state`, so if the core cannot remember that the user finished, showing the
 * flow would trap them in it on every launch. Until the Rust side lands, first run is reachable
 * from the browser preview, and on device the panel opens as it does today. */
export function getOnboardingState(): Promise<OnboardingState> {
  if (!IN_TAURI) return Promise.resolve({ ...FIRST_RUN, completed: true });
  return invoke<OnboardingState>("onboarding_state")
    .then((s) => ({ ...FIRST_RUN, ...s }))
    .catch(() => ({ ...FIRST_RUN, completed: true }));
}

/** (new) `set_onboarding_state(step, plan, completed)` — a whole-record write; the flow has one
 *  writer, and a partial update would let a resumed session disagree with itself. */
export function setOnboardingState(next: OnboardingState): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("set_onboarding_state", {
    step: next.step,
    plan: next.plan,
    completed: next.completed,
  }).catch(() => undefined);
}

/** (new) `ax_permission() -> bool` — whether this process is trusted for Accessibility.
 *  Wraps the existing `axcache::ax_trusted()`, WITHOUT the prompt option: polling a prompting
 *  check would reopen the system dialog every second. */
export function axPermission(): Promise<boolean> {
  return ask<boolean>("ax_permission", {}, false);
}

/** (new) `request_ax_permission()` — the prompting variant, fired once from the button, and it
 *  opens System Settings at the right pane when the prompt has already been answered. */
export function requestAxPermission(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("request_ax_permission").catch(() => undefined);
}

/** (new) `exclusion_categories() -> ExclusionCategory[]` — read from the live ExclusionPolicy so
 *  onboarding shows what is actually enforced. */
export function exclusionCategories(): Promise<ExclusionCategory[]> {
  return ask<ExclusionCategory[]>("exclusion_categories", {}, [
    { id: "password_managers", count: 6 },
    { id: "auth_dialog", count: 1 },
    { id: "terminals", count: 5 },
    { id: "private_browsing", count: 4 },
  ]);
}

/** (new) `get_draft_stop() -> bool` / `set_draft_stop(enabled)` — the drafts-only mode required by
 *  CLAUDE.md. Default ON: nothing sends until the user turns that off AND confirms each send. */
export function getDraftStop(): Promise<boolean> {
  return ask<boolean>("get_draft_stop", {}, true);
}
export function setDraftStop(enabled: boolean): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("set_draft_stop", { enabled }).catch(() => undefined);
}
