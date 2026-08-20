import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GateFrame } from "./experience/GateFrame";
import { OnboardingExperience } from "./experience/OnboardingExperience";
import { AmbientSurface } from "./experience/AmbientSurface";
import { newestPermissionSnapshot, Onboarding, windowRoute } from "./Onboarding";
import type { OnboardingMotionVector, PermissionSnapshot } from "./ipc";

const emptyPermissions: PermissionSnapshot = {
  accessibility: false, microphone: false, screen_recording: false, all_granted: false,
  accessibility_state: "not_granted", microphone_state: "not_determined", screen_recording_state: "not_granted",
  all_effective: false, reason: null, revision: 1,
};
const state = (step: string, revision = 1) => ({ completed: false, step, plan: null, revision, intro_complete: true, music_muted: false });

beforeEach(() => { vi.clearAllMocks(); window.history.replaceState({}, "", "/onboarding.html"); });
afterEach(cleanup);

function mockNative(initialStep: string, permissions = emptyPermissions): void {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "onboarding_state") return state(initialStep);
    if (command === "permission_status" || command === "permission_listener_ready") return permissions;
    if (command === "onboarding_event" || command === "get_shortcuts") return command === "get_shortcuts" ? {} : undefined;
    throw new Error(`unexpected command: ${command}`);
  });
}

describe("cinematic onboarding", () => {
  it("routes native surfaces and rejects an invalid route", () => {
    expect(windowRoute("?surface=ambient&generation=8")).toEqual({ surface: "ambient", generation: 8 });
    expect(windowRoute("?surface=wrong&generation=x")).toEqual({ surface: "interactive", generation: null });
  });

  it("renders native main and ambient routes only after generation validation", async () => {
    window.history.replaceState({}, "", "/onboarding.html?surface=main&generation=9");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("welcome");
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_window_surface") return { surface: "main", generation: 9, display_id: 1, motion_vector: { x: 0, y: 0 }, label: "onboarding-main-9" };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    const view = render(<Onboarding />);
    expect(await screen.findByTestId("cinematic-surface")).toBeTruthy();
    view.unmount();
    window.history.replaceState({}, "", "/onboarding.html?surface=ambient&generation=9");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("welcome");
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_window_surface") return { surface: "ambient", generation: 9, display_id: 2, motion_vector: { x: 1, y: -1 }, label: "onboarding-ambient-9" };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const ambient = await screen.findByTestId("ambient-surface");
    expect(ambient.getAttribute("data-motion-x")).toBe("1");
    expect(ambient.getAttribute("data-motion-y")).toBe("-1");
  });

  it("maps every bounded native motion component onto the ambient surface", () => {
    const view = render(<AmbientSurface motionVector={{ x: -1, y: 1 }} />);
    const ambient = screen.getByTestId("ambient-surface");
    expect(ambient.getAttribute("data-motion-x")).toBe("-1");
    expect(ambient.getAttribute("data-motion-y")).toBe("1");
    view.rerender(
      <AmbientSurface motionVector={{ x: 7, y: -4 } as unknown as OnboardingMotionVector} />,
    );
    expect(ambient.getAttribute("data-motion-x")).toBe("1");
    expect(ambient.getAttribute("data-motion-y")).toBe("-1");
  });

  it("rejects stale generation surface mismatch", async () => {
    window.history.replaceState({}, "", "/onboarding.html?surface=interactive&generation=9");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("welcome");
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_window_surface") return null;
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByTestId("stale-surface")).toBeTruthy();
  });

  it("never regresses permission state to an older native revision", () => {
    const newer = { ...emptyPermissions, revision: 8, accessibility: true };
    expect(newestPermissionSnapshot(newer, { ...emptyPermissions, revision: 7 })).toBe(newer);
  });

  it("hydrates semantic state without welcome flash", async () => {
    mockNative("microphone");
    render(<Onboarding />);
    expect(await screen.findByRole("heading", { name: "Microphone" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Make room for your work." })).toBeNull();
  });

  it("shows exactly current permission and persists only after native grant", async () => {
    mockNative("accessibility");
    render(<Onboarding />);
    expect(await screen.findByRole("heading", { name: "Accessibility" })).toBeTruthy();
    expect(screen.queryByText("Microphone")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Allow" }));
    expect(invoke).toHaveBeenCalledWith("open_accessibility_settings");
    expect(invoke).not.toHaveBeenCalledWith("set_onboarding_state", expect.anything());
  });

  it("auto-advances one time only after a matching granted snapshot and save resolves", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("accessibility", 4);
      if (command === "permission_status" || command === "permission_listener_ready") return { ...emptyPermissions, accessibility: true, revision: 5 };
      if (command === "set_onboarding_state") return state("microphone", 5);
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 4, step: "microphone", plan: null, completed: false }));
    expect(await screen.findByRole("heading", { name: "Microphone" })).toBeTruthy();
  });

  it("persists Mute through native revision CAS and renders saved state", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("welcome", 4);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "set_onboarding_music_muted") return { ...state("welcome", 5), music_muted: true };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    fireEvent.click(await screen.findByRole("button", { name: "Mute" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_music_muted", { expectedRevision: 4, muted: true }));
    expect(screen.getByRole("button", { name: "Unmute" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("serializes Mute clicks while the native CAS is pending", async () => {
    let resolveMute: ((saved: ReturnType<typeof state>) => void) | undefined;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("welcome", 4);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "set_onboarding_music_muted") return new Promise<ReturnType<typeof state>>((resolve) => { resolveMute = resolve; });
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const mute = await screen.findByRole("button", { name: "Mute" });
    fireEvent.click(mute);
    expect((mute as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(mute);
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "set_onboarding_music_muted")).toHaveLength(1);
    resolveMute?.({ ...state("welcome", 5), music_muted: true });
    expect((await screen.findByRole("button", { name: "Unmute" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("keeps native Mute available during the cinematic surface", async () => {
    window.history.replaceState({}, "", "/onboarding.html?surface=main&generation=9");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("welcome", 4);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_window_surface") return { surface: "main", generation: 9, display_id: 1, motion_vector: { x: 0, y: 0 }, label: "onboarding-main-9" };
      if (command === "set_onboarding_music_muted") return { ...state("welcome", 5), music_muted: true };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByTestId("cinematic-surface")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Mute" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_music_muted", { expectedRevision: 4, muted: true }));
    expect(screen.getByRole("button", { name: "Unmute" })).toBeTruthy();
  });

  it("keeps Screen Recording on stage when restart fails", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("screen_recording", 4);
      if (command === "permission_status" || command === "permission_listener_ready") return { ...emptyPermissions, screen_recording_state: "restart_required" };
      if (command === "restart_onboarding") throw new Error("cannot restart");
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    fireEvent.click(await screen.findByRole("button", { name: "Restart SHOGUN" }));
    expect(await screen.findByText("Restart did not begin. Keep this window open and try again.")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Screen Recording" })).toBeTruthy();
  });

  it("keeps Screen Recording on stage when restart begins", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("screen_recording", 4);
      if (command === "permission_status" || command === "permission_listener_ready") return { ...emptyPermissions, screen_recording_state: "restart_required" };
      if (command === "restart_onboarding" || command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    fireEvent.click(await screen.findByRole("button", { name: "Restart SHOGUN" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("restart_onboarding", { expectedRevision: 4, step: "screen_recording" }));
    expect(screen.queryByText("Restart did not begin. Keep this window open and try again.")).toBeNull();
    expect(screen.getByRole("heading", { name: "Screen Recording" })).toBeTruthy();
  });

  it("keeps the still mounted, removes gate copy, and plays completion video once", () => {
    const { rerender } = render(<GateFrame />);
    const gate = screen.getByTestId("gate-frame");
    const image = screen.getByRole("img", { name: "A wooden gate opening onto an autumn path" });
    expect(image.getAttribute("src")).toContain("gate-autumn-path");
    expect(image.getAttribute("width")).toBe("1024");
    expect(image.getAttribute("height")).toBe("1536");
    expect(image.classList.contains("onb-gate__image")).toBe(true);
    expect(image.closest(".onb-gate__picture")).toBeTruthy();
    expect(screen.queryByTestId("gate-opening-video")).toBeNull();
    expect(screen.queryByText("Gate awaits")).toBeNull();
    expect(screen.queryByText("Your workspace, within reach.")).toBeNull();
    rerender(<GateFrame complete />);
    expect(screen.getByTestId("gate-frame")).toBe(gate);
    expect(gate.getAttribute("data-complete")).toBe("true");
    expect(gate.classList.contains("onb-gate--frame")).toBe(true);
    const video = screen.getByTestId("gate-opening-video") as HTMLVideoElement;
    expect(video.autoplay).toBe(true);
    expect(video.muted).toBe(true);
    expect(video.playsInline).toBe(true);
    expect(video.loop).toBe(false);
    expect(video.getAttribute("poster")).toContain("gate-autumn-path");
    expect(video.querySelector("source")?.getAttribute("src")).toContain("gate-opening");
    rerender(<GateFrame variant="full-window" />);
    expect(screen.getByTestId("gate-frame").classList.contains("onb-gate--full-window")).toBe(true);
  });

  it("keeps the completion gate still under reduced motion", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    render(<GateFrame complete />);
    expect(screen.queryByTestId("gate-opening-video")).toBeNull();
    expect(screen.getByRole("img", { name: "A wooden gate opening onto an autumn path" })).toBeTruthy();
    vi.unstubAllGlobals();
  });

  it("plays the gate before saving completion and retains floating Mute", async () => {
    const onFinish = vi.fn(async () => true);
    render(<OnboardingExperience state={{ ...state("gate"), step: "gate" }} permissions={{ ...emptyPermissions, all_effective: true }} surfaceGeneration={1} onPersist={vi.fn(async () => true)} onFinish={onFinish} onToggleMusic={vi.fn(async () => true)} musicPending={false} />);
    expect(document.querySelector(".onb-header")).toBeNull();
    expect(screen.getByRole("button", { name: "Mute" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(onFinish).not.toHaveBeenCalled();
    fireEvent.ended(screen.getByTestId("gate-opening-video"));
    await waitFor(() => expect(onFinish).toHaveBeenCalledTimes(1));
    fireEvent.error(screen.getByTestId("gate-opening-video"));
    expect(onFinish).toHaveBeenCalledTimes(1);
  });

  it("finishes safely on gate playback error", async () => {
    const onFinish = vi.fn(async () => true);
    render(<OnboardingExperience state={{ ...state("gate"), step: "gate" }} permissions={{ ...emptyPermissions, all_effective: true }} surfaceGeneration={1} onPersist={vi.fn(async () => true)} onFinish={onFinish} onToggleMusic={vi.fn(async () => true)} musicPending={false} />);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(onFinish).not.toHaveBeenCalled();
    fireEvent.error(screen.getByTestId("gate-opening-video"));
    await waitFor(() => expect(onFinish).toHaveBeenCalledTimes(1));
  });

  it("skips gate playback and finishes immediately under reduced motion", async () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    const onFinish = vi.fn(async () => true);
    render(<OnboardingExperience state={{ ...state("gate"), step: "gate" }} permissions={{ ...emptyPermissions, all_effective: true }} surfaceGeneration={1} onPersist={vi.fn(async () => true)} onFinish={onFinish} onToggleMusic={vi.fn(async () => true)} musicPending={false} />);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(onFinish).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("gate-opening-video")).toBeNull();
    vi.unstubAllGlobals();
  });

  it("keeps live privacy exclusions and advances only after Rust save", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("privacy", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "exclusion_categories") return [{ id: "terminals", count: 5 }];
      if (command === "set_onboarding_state") return state("plan", 9);
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByText("Never read at all")).toBeTruthy();
    expect(screen.getByText("Terminals")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 8, step: "plan", plan: null, completed: false }));
  });

  it("keeps Pro Keychain entry, pressed plan semantics, and plan skip", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return { ...state("plan", 8), plan: "pro" };
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "set_byok_key") return undefined;
      if (command === "set_onboarding_state") return state("connect", 9);
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const pro = await screen.findByRole("button", { name: /Pro/ });
    expect(pro.getAttribute("aria-pressed")).toBe("true");
    fireEvent.change(screen.getByLabelText("Your key"), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_byok_key", { provider: "anthropic", key: "secret" }));
    fireEvent.click(screen.getByRole("button", { name: "Skip for now" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 8, step: "connect", plan: "pro", completed: false }));
  });

  it("shows draft-stop as locked status and persists analytics in both directions", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("connect", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "composio_settings") return { draft_stop: true, consent_acknowledged: false };
      if (command === "analytics_get_opt_out") return false;
      if (command === "analytics_set_opt_out") return undefined;
      if (command === "hotkey") return undefined;
      if (command === "connectors_list") return [];
      if (command === "set_onboarding_state") return state("gate", 9);
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByRole("status", { name: /Drafts only/ })).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: /Drafts only/ })).toBeNull();
    expect(screen.getByText("Turning this off needs your consent first — that lives in Settings.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("hotkey"));
    expect(emit).toHaveBeenCalledWith("open-onboarding-settings", { section: "connections" });
    const analytics = await screen.findByRole("checkbox", { name: /Share anonymous usage metrics/ });
    expect((analytics as HTMLInputElement).checked).toBe(true);
    fireEvent.click(analytics);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("analytics_set_opt_out", { optOut: true }));
    expect((analytics as HTMLInputElement).checked).toBe(false);
    fireEvent.click(analytics);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("analytics_set_opt_out", { optOut: false }));
    expect((analytics as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Skip for now" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 8, step: "gate", plan: null, completed: false }));
  });

  it("keeps analytics visibly off when its persisted state cannot be read", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("connect", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "analytics_get_opt_out") throw new Error("unavailable");
      if (command === "connectors_list") return [];
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const analytics = await screen.findByRole("checkbox", { name: /Share anonymous usage metrics/ });
    expect((analytics as HTMLInputElement).checked).toBe(false);
    expect(screen.getByRole("status", { name: "Usage settings are unavailable. Sharing remains off." })).toBeTruthy();
  });

  it("does not turn browser key events into shortcut success", async () => {
    mockNative("dictation_demo");
    render(<Onboarding />);
    const field = await screen.findByRole("textbox", { name: "Dictation practice field" });
    fireEvent.keyDown(field, { key: "v", ctrlKey: true, altKey: true });
    fireEvent.keyUp(field, { key: "v", ctrlKey: true, altKey: true });
    expect(screen.getByText("Waiting for native dictation result. Copied or failed text stays a retry.")).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("set_onboarding_state", expect.anything());
  });

  it("advances dictation only from matching inserted delivery", async () => {
    let handler!: (event: { payload: { generation: number; nonce: string; stage: "dictation_demo"; session_id: number | null; outcome: string } }) => void;
    vi.mocked(listen).mockImplementation(async (_event, callback) => { handler = callback as typeof handler; return () => undefined; });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("dictation_demo", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_shortcut_arm") return { generation: 7, nonce: "voice", stage: "dictation_demo", binding: "Control+Shift+KeyD", supports_demo: true, voice_enabled: true };
      if (command === "onboarding_shortcut_ready" || command === "onboarding_event") return undefined;
      if (command === "get_shortcuts") return { voice: "Control+Shift+KeyD" };
      if (command === "set_onboarding_state") return state("plan", 9);
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_ready", { generation: 7, nonce: "voice", surfaceGeneration: 0 }));
    handler({ payload: { generation: 7, nonce: "voice", stage: "dictation_demo", session_id: 12, outcome: "dictation_copied" } });
    expect(invoke).not.toHaveBeenCalledWith("set_onboarding_state", expect.anything());
    handler({ payload: { generation: 7, nonce: "wrong", stage: "dictation_demo", session_id: 12, outcome: "dictation_inserted" } });
    expect(invoke).not.toHaveBeenCalledWith("set_onboarding_state", expect.anything());
    handler({ payload: { generation: 7, nonce: "voice", stage: "dictation_demo", session_id: 12, outcome: "dictation_inserted" } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 8, step: "plan", plan: null, completed: false }));
  });

  it("restores unsupported Scribe binding only after explicit click", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("scribe_demo", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_shortcut_arm") return { generation: 4, nonce: "scribe", stage: "scribe_demo", binding: "Control+Shift+KeyR", supports_demo: false, supports_scribe: false };
      if (command === "onboarding_shortcut_disarm" || command === "onboarding_event") return undefined;
      if (command === "set_shortcut") return undefined;
      if (command === "get_shortcuts") return { draft: "Control+Shift+KeyR" };
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const restore = await screen.findByRole("button", { name: "Restore Right Option" });
    expect(invoke).not.toHaveBeenCalledWith("set_shortcut", expect.anything());
    fireEvent.click(restore);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_shortcut", { action: "draft", combo: "Tap+Alt" }));
  });

  it("keeps native Scribe field content and Try Again performs disarm plus re-arm", async () => {
    let handler!: (event: { payload: { generation: number; nonce: string; stage: "scribe_demo"; session_id: number | null; outcome: "cancelled" } }) => void;
    let armCount = 0;
    vi.mocked(listen).mockImplementation(async (_event, callback) => { handler = callback as typeof handler; return () => undefined; });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("scribe_demo", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_shortcut_arm") {
        armCount += 1;
        return { generation: armCount, nonce: `scribe-${armCount}`, stage: "scribe_demo", binding: "Tap+Alt", supports_demo: true, supports_scribe: true };
      }
      if (command === "onboarding_shortcut_ready" || command === "onboarding_shortcut_disarm" || command === "onboarding_event") return undefined;
      if (command === "get_shortcuts") return { draft: "Tap+Alt" };
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const field = await screen.findByRole("textbox", { name: "Sample email" }) as HTMLTextAreaElement;
    fireEvent.change(field, { target: { value: "native AX result" } });
    handler({ payload: { generation: 1, nonce: "scribe-1", stage: "scribe_demo", session_id: 4, outcome: "cancelled" } });
    expect(await screen.findByText("That attempt did not land in this field. Try again.")).toBeTruthy();
    expect(field.value).toBe("native AX result");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_disarm", { generation: 1, nonce: "scribe-1" }));
    await waitFor(() => expect(armCount).toBe(2));
  });

  it("disarms a scope whose arm resolves after unmount", async () => {
    let resolveArm!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "onboarding_state") return Promise.resolve(state("right_option", 8));
      if (command === "permission_status" || command === "permission_listener_ready") return Promise.resolve(emptyPermissions);
      if (command === "onboarding_shortcut_arm") return new Promise((resolve) => { resolveArm = resolve; });
      if (command === "onboarding_shortcut_disarm" || command === "onboarding_event") return Promise.resolve(undefined);
      if (command === "get_shortcuts") return Promise.resolve({});
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const view = render(<Onboarding />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_arm", { expectedRevision: 8, step: "right_option" }));
    view.unmount();
    resolveArm({ generation: 9, nonce: "late", stage: "right_option", binding: "Tap+Alt", supports_demo: true, supports_scribe: true });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_disarm", { generation: 9, nonce: "late" }));
  });

  it("advances practice only from matching native shortcut proof", async () => {
    let handler!: (event: { payload: { generation: number; nonce: string; stage: "right_option"; session_id: number | null; outcome: "single_tap" } }) => void;
    vi.mocked(listen).mockImplementation(async (_event, callback) => { handler = callback as typeof handler; return () => undefined; });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("right_option", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_shortcut_arm") return { generation: 4, nonce: "nonce", stage: "right_option", binding: "Tap+Alt" };
      if (command === "onboarding_shortcut_ready" || command === "onboarding_event" || command === "get_shortcuts") return command === "get_shortcuts" ? {} : undefined;
      if (command === "set_onboarding_state") return state("scribe_demo", 9);
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_arm", { expectedRevision: 8, step: "right_option" }));
    handler({ payload: { generation: 4, nonce: "wrong", stage: "right_option", session_id: null, outcome: "single_tap" } });
    expect(invoke).not.toHaveBeenCalledWith("set_onboarding_state", expect.anything());
    handler({ payload: { generation: 4, nonce: "nonce", stage: "right_option", session_id: null, outcome: "single_tap" } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 8, step: "scribe_demo", plan: null, completed: false }));
  });

  it("waits for the native shortcut listener before arming or readying", async () => {
    let resolveShortcutListener!: (off: () => void) => void;
    vi.mocked(listen).mockImplementation((event) => event === "onboarding-shortcut"
      ? new Promise<() => void>((resolveListener) => { resolveShortcutListener = resolveListener; })
      : Promise.resolve(() => undefined));
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("right_option", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "onboarding_shortcut_arm") return { generation: 4, nonce: "nonce", stage: "right_option", binding: "AltRight" };
      if (command === "onboarding_shortcut_ready" || command === "onboarding_event" || command === "get_shortcuts") return command === "get_shortcuts" ? {} : undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByRole("heading", { name: "Find Right Option." })).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("onboarding_shortcut_arm", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("onboarding_shortcut_ready", expect.anything());
    resolveShortcutListener(() => undefined);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_arm", { expectedRevision: 8, step: "right_option" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_shortcut_ready", { generation: 4, nonce: "nonce", surfaceGeneration: 0 }));
  });

  it("keeps cinematic motion compositor-only and reduced motion opacity-only", () => {
    const css = readFileSync(resolve(process.cwd(), "src/onboarding/onboarding.css"), "utf8");
    const cinematicSource = readFileSync(resolve(process.cwd(), "src/onboarding/experience/CinematicSurface.tsx"), "utf8");
    const keyframes = css.split("\n").filter((line) => line.startsWith("@keyframes")).join("\n");
    const reduced = css.split("\n").find((line) => line.startsWith("@media (prefers-reduced-motion: reduce)")) ?? "";
    const reducedFade = css.split("\n").find((line) => line.startsWith("@keyframes onb-reduced-current-fade")) ?? "";
    expect(css.includes("onb-haze")).toBe(false);
    expect(css.includes("onb-header")).toBe(false);
    expect(css.includes("onb-gate__legend")).toBe(false);
    expect(css).not.toMatch(/transition\s*:\s*all/i);
    expect(css).not.toContain("Fraunces Onboarding");
    expect(css).not.toContain("Avenir Next");
    expect(css).not.toMatch(/@font-face|\.ttf|\.woff/i);
    expect(css).toContain('--onb-font-ui: system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text"');
    expect(css).toContain('--onb-font-rounded: ui-rounded, ".SF NS Rounded", "SF Pro Rounded"');
    expect(css).toContain('--onb-font-mono: ui-monospace, "SFMono-Regular", "SF Mono"');
    expect(css).toContain("--onb-text-large-title-size: 26px");
    expect(css).toContain("--onb-text-body-size: 13px");
    expect(css).toContain("--onb-text-button-default-size: 13px");
    expect(css).toMatch(/\.onb-cinematic\s*\{[^}]*background:\s*rgba\(/);
    expect(css).toMatch(/\.onb-layout\s*\{[^}]*min-height:\s*0/);
    expect(css).toMatch(/@media \(max-width:\s*760px\)/);
    expect(css).toContain("onb-light-gather 4s");
    expect(css).toContain("onb-white-bloom 4s");
    expect(css).toContain("rgba(195,95,60,.58)");
    expect(css).toContain("rgba(95,143,168,.34)");
    expect(css).not.toMatch(/violet|yellow/i);
    expect(cinematicSource).toContain("onb-cinematic__light--ember");
    expect(cinematicSource).toContain("onb-cinematic__light--glacier");
    expect(cinematicSource).not.toMatch(/Shotbase|wavesUrl|<Logo/);
    expect(css).not.toMatch(/onb-(?:button|mute|drag)[^}]*min-height:\s*(?:3[0-9]|4[0-3])px/);
    expect(keyframes).not.toMatch(/\b(width|height|top|right|bottom|left|margin|padding)\s*:/i);
    expect(reduced).toContain("onb-reduced-current-fade 200ms linear both");
    expect(reduced).toContain("onb-reduced-light-fade 200ms linear both");
    expect(reduced).toContain("onb-reduced-bloom-fade 200ms linear both");
    expect(reduced).not.toContain("display: none");
    expect(reduced).not.toContain("onb-light-gather");
    expect(reduced).not.toContain("onb-white-bloom");
    expect(reduced).not.toContain("onb-ambient-flow");
    expect(reducedFade).toMatch(/opacity:/);
    expect(reducedFade).not.toMatch(/transform:/);
    const ambientSource = readFileSync(resolve(process.cwd(), "src/onboarding/experience/AmbientSurface.tsx"), "utf8");
    expect(ambientSource).not.toContain("requestAnimationFrame");
  });

  it("renders custom native binding honestly", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("right_option");
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "get_shortcuts") return { draft: "Control+Shift+KeyR" };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByText("R")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Use ⌃ + ⇧ + R." })).toBeTruthy();
    expect(screen.getByText("Use ⌃ + ⇧ + R once to prepare a draft where your cursor is.")).toBeTruthy();
  });

  it.each([
    ["intro", "Make room for your work."],
    ["welcome", "Make room for your work."],
    ["reads", "What it reads, and what it never keeps."],
    ["privacy", "What it reads, and what it never keeps."],
    ["accessibility", "Accessibility"],
    ["microphone", "Microphone"],
    ["screen_recording", "Screen Recording"],
    ["right_option", "Find Right Option."],
    ["scribe_demo", "Make a rough note clean."],
    ["dictation_demo", "Speak into the field."],
    ["plan", "Seven days of everything."],
    ["connect", "Connect what you work in."],
    ["gate", "Setup is ready."],
    ["ready", "Setup is ready."],
  ])("hydrates semantic %s state without a welcome flash", async (step, heading) => {
    mockNative(step);
    render(<Onboarding />);
    expect(await screen.findByRole("heading", { name: heading })).toBeTruthy();
    if (heading !== "Make room for your work.") expect(screen.queryByRole("heading", { name: "Make room for your work." })).toBeNull();
  });
});
