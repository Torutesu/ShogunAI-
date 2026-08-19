// The onboarding flow's contract with the Rust core.
//
// Every question onboarding asks about the machine — is Accessibility granted, what is never
// read, is a key present, is the drafts-only mode on — is answered by Rust, not decided here
// (invariant 1). This module is the single list of what the core provides, so the flow itself
// stays presentation-only.
//
// Command mapping (kept aligned with src-tauri):
// - onboarding_state / set_onboarding_state ........ onboarding.rs (Rust-owned progress + trial)
// - permission_status .............................. onboarding.rs (NON-prompting snapshot)
// - open/request permission commands ............... onboarding.rs (explicit user actions only)
// - exclusion_categories ........................... exclusions.rs (live policy, never hardcoded)
// - composio_settings / set_composio_policy ........ approvals.rs — draft-stop's single source of
//   truth is ComposioPolicy (composio.json). Onboarding does NOT keep its own copy; turning it
//   off without consent is rejected by Rust and the toggle falls back to ON (invariant 4).
// - onboarding_event ............................... onboarding.rs → #91 PostHog adapter, behind
//   its opt_out gate; names are allowlisted on the Rust side.

import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";

// Use Tauri's public runtime detector. Private globals changed across Tauri 2 releases and can be
// absent even while IPC works, which made the permission guide poll its browser fallback forever.
export const IN_TAURI = isTauri();

/** Bring forward SHOGUN's existing panel and route its frontend to Connections settings. */
export async function openOnboardingSettings(): Promise<boolean> {
  if (!IN_TAURI) return false;
  try {
    await invoke("hotkey");
    await emit("open-onboarding-settings", { section: "connections" });
    return true;
  } catch {
    return false;
  }
}

/** Invoke that degrades to a safe, honest fallback outside Tauri (`pnpm dev:vite` in a browser)
 *  or when a command fails — the flow then reads as "not granted / nothing connected", never as
 *  a fabricated success. */
function ask<T>(cmd: string, args: Record<string, unknown>, fallback: T): Promise<T> {
  if (!IN_TAURI) return Promise.resolve(fallback);
  return invoke<T>(cmd, args).catch(() => fallback);
}

/** Where the user got to. Persisted by Rust so a quit mid-flow resumes, not restarts. */
export type StepId = "welcome" | "reads" | "permission" | "plan" | "connect" | "ready";

export type SemanticStepId =
  | StepId
  | "intro"
  | "accessibility"
  | "microphone"
  | "screen_recording"
  | "right_option"
  | "scribe_demo"
  | "dictation_demo"
  | "privacy"
  | "gate";

export const STEPS: StepId[] = ["welcome", "reads", "permission", "plan", "connect", "ready"];

export function isCurrentStep(step: SemanticStepId): step is StepId {
  return (STEPS as SemanticStepId[]).includes(step);
}

export interface OnboardingState {
  /** True once the user has finished (or explicitly skipped to the end). */
  completed: boolean;
  /** The furthest step reached, so quitting mid-flow resumes there. */
  step: SemanticStepId;
  /** Compare-and-set revision returned by Rust. */
  revision: number;
  intro_complete: boolean;
  music_muted: boolean;
  restart_pending?: {
    reason: "screen_recording";
    bundle_id: string;
    step: SemanticStepId;
  } | null;
  /** Which plan the user said they wanted. Billing is a separate flow; this only decides whether
   *  onboarding asks for a key. Real entitlement gating is a Rust-core follow-up. */
  plan: "standard" | "pro" | null;
  /** Present only when a required Mac permission was lost after setup. */
  permissions_repair?: boolean;
}

export interface OnboardingMotionVector {
  x: -1 | 0 | 1;
  y: -1 | 0 | 1;
}

export interface OnboardingWindowSurface {
  surface: "main" | "ambient" | "interactive";
  generation: number;
  display_id: number;
  motion_vector: OnboardingMotionVector;
  label: string;
}

/** Native-owned surface identity. Query parameters select initial route; this typed answer fences
 * delayed frontend work to exact window-session generation. */
export function onboardingWindowSurface(
  expectedGeneration: number,
): Promise<OnboardingWindowSurface | null> {
  return ask<OnboardingWindowSurface | null>(
    "onboarding_window_surface",
    { expectedGeneration },
    null,
  );
}

/** Side-effect-free status for every capability required by the first-run permission center. */
export interface PermissionSnapshot {
  accessibility: boolean;
  microphone: boolean;
  screen_recording: boolean;
  all_granted: boolean;
  accessibility_state: "granted" | "not_granted";
  microphone_state: "not_determined" | "denied" | "restricted" | "granted";
  screen_recording_state: "not_granted" | "granted" | "restart_required";
  all_effective: boolean;
  reason:
    | "screen_recording_restart_required"
    | "screen_recording_settings_repair_pending"
    | null;
  revision: number;
}

export const EMPTY_PERMISSIONS: PermissionSnapshot = {
  accessibility: false,
  microphone: false,
  screen_recording: false,
  all_granted: false,
  accessibility_state: "not_granted",
  microphone_state: "not_determined",
  screen_recording_state: "not_granted",
  all_effective: false,
  reason: null,
  revision: 0,
};

/** A category of thing SHOGUN never reads. Rust owns the list — the UI must not hardcode it,
 *  because a UI that lists yesterday's exclusions is a lie about today's behaviour. */
export interface ExclusionCategory {
  /** Stable id, looked up in the string catalogue for its label. */
  id: string;
  /** How many bundle ids / rules the category covers, for "6 apps" style copy. */
  count: number;
}

const FIRST_RUN: OnboardingState = {
  completed: false,
  step: "welcome",
  revision: 0,
  intro_complete: false,
  music_muted: false,
  restart_pending: null,
  plan: null,
};

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
export function setOnboardingState(next: OnboardingState): Promise<OnboardingState | null> {
  if (!IN_TAURI) return Promise.resolve({ ...next, revision: next.revision + 1 });
  return invoke<OnboardingState>("set_onboarding_state", {
    expectedRevision: next.revision,
    step: next.step,
    plan: next.plan,
    completed: next.completed,
  }).then(
    (saved) => ({ ...FIRST_RUN, ...saved }),
    () => null,
  );
}

/** Persist Mute through the same Rust-owned revision gate as onboarding progress. */
export function setOnboardingMusicMuted(
  state: OnboardingState,
  muted: boolean,
): Promise<OnboardingState | null> {
  if (!IN_TAURI) return Promise.resolve({ ...state, music_muted: muted, revision: state.revision + 1 });
  return invoke<OnboardingState>("set_onboarding_music_muted", {
    expectedRevision: state.revision,
    muted,
  }).then(
    (saved) => ({ ...FIRST_RUN, ...saved }),
    () => null,
  );
}

/** Persist the exact Screen Recording step, then relaunch the packaged app. */
export function restartOnboarding(state: OnboardingState): Promise<void> {
  if (!IN_TAURI) return Promise.reject(new Error("Restart requires the packaged app"));
  return invoke<void>("restart_onboarding", {
    expectedRevision: state.revision,
    step: state.step,
  });
}

/** Clear a restart marker only after this exact step has rendered and native access is effective. */
export function acknowledgeOnboardingRestart(
  state: OnboardingState,
): Promise<OnboardingState | null> {
  if (!IN_TAURI) return Promise.resolve({ ...state, restart_pending: null });
  return invoke<OnboardingState>("acknowledge_onboarding_restart", {
    expectedRevision: state.revision,
    step: state.step,
  }).then(
    (saved) => ({ ...FIRST_RUN, ...saved }),
    () => null,
  );
}

/** `permission_status() -> PermissionSnapshot` — every check is NON-prompting. */
export function permissionStatus(): Promise<PermissionSnapshot> {
  return ask<PermissionSnapshot>("permission_status", {}, EMPTY_PERMISSIONS);
}

/** Listener-ready handshake. Rust emits one initial snapshot only after this command follows a
 * resolved `listen`, while `permissionStatus` remains authoritative bootstrap. */
export function permissionListenerReady(): Promise<PermissionSnapshot | null> {
  if (!IN_TAURI) return Promise.resolve(null);
  return invoke<PermissionSnapshot>("permission_listener_ready").catch(() => null);
}

/** `open_accessibility_settings()` — the prompting variant, fired once from the button; also
 *  opens System Settings at the right pane for when the prompt has already been answered. */
export function requestAxPermission(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("open_accessibility_settings").catch(() => undefined);
}

/** Ask macOS for microphone access, or open the repair pane after a prior denial. */
export function requestMicrophonePermission(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("request_microphone_permission").catch(() => undefined);
}

/** Ask macOS for Screen Recording, or open the repair pane after a prior denial. */
export function requestScreenRecordingPermission(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("request_screen_recording_permission").catch(() => undefined);
}

/** Arm/disarm the native app-bundle drag used by Accessibility and Screen Recording panes. */
export function armPermissionDrag(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("arm_permission_app_drag").catch(() => undefined);
}

export function disarmPermissionDrag(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("disarm_permission_app_drag").catch(() => undefined);
}

/** Live bindings are Rust-owned. Practice UI may render them, never infer a default as proof. */
export function getShortcuts(): Promise<Record<string, string>> {
  return ask<Record<string, string>>("get_shortcuts", {}, {});
}

export type OnboardingShortcutStage = "right_option" | "scribe_demo" | "dictation_demo";
export type OnboardingShortcutArm = {
  generation: number;
  nonce: string;
  stage: OnboardingShortcutStage;
  binding: string;
  supports_demo: boolean;
  supports_scribe: boolean;
  voice_enabled: boolean;
  seeded_text?: string | null;
};
export type OnboardingShortcutEvent = {
  generation: number;
  nonce: string;
  stage: OnboardingShortcutStage;
  session_id: number | null;
  outcome: "single_tap" | "scribe_opened" | "scribe_inserted" | "dictation_inserted" | "dictation_copied" | "no_key" | "failed" | "cancelled" | "stale";
};

/** Native tutorial coordinator owns proof. Browser key events can never complete practice. */
export function onboardingShortcutArm(expectedRevision: number, step: OnboardingShortcutStage): Promise<OnboardingShortcutArm | null> {
  return ask<OnboardingShortcutArm | null>("onboarding_shortcut_arm", { expectedRevision, step }, null);
}
export function onboardingShortcutReady(generation: number, nonce: string, surfaceGeneration: number): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("onboarding_shortcut_ready", { generation, nonce, surfaceGeneration }).catch(() => undefined);
}
export function onboardingShortcutDisarm(generation: number, nonce: string): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke<void>("onboarding_shortcut_disarm", { generation, nonce }).catch(() => undefined);
}

export function restoreOnboardingShortcut(action: "draft" | "voice", combo: string): Promise<boolean> {
  if (!IN_TAURI) return Promise.resolve(false);
  return invoke<void>("set_shortcut", { action, combo }).then(() => true).catch(() => false);
}

export function enableOnboardingDictation(): Promise<boolean> {
  if (!IN_TAURI) return Promise.resolve(false);
  return invoke<void>("set_voice_enabled", { enabled: true }).then(() => true).catch(() => false);
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
