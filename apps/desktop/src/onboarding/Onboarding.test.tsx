import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GateFrame } from "./experience/GateFrame";
import { newestPermissionSnapshot, Onboarding, windowRoute } from "./Onboarding";
import type { PermissionSnapshot } from "./ipc";

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
      if (command === "onboarding_window_surface") return { surface: "main", generation: 9, display_id: 1, label: "onboarding-main-9" };
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
      if (command === "onboarding_window_surface") return { surface: "ambient", generation: 9, display_id: 2, label: "onboarding-ambient-9" };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    expect(await screen.findByTestId("ambient-surface")).toBeTruthy();
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

  it("keeps gate frame mounted and leaves full-window API unreachable", () => {
    const { rerender } = render(<GateFrame />);
    const gate = screen.getByTestId("gate-frame");
    rerender(<GateFrame complete />);
    expect(screen.getByTestId("gate-frame")).toBe(gate);
    expect(gate.getAttribute("data-complete")).toBe("true");
    expect(gate.classList.contains("onb-gate--frame")).toBe(true);
    rerender(<GateFrame variant="full-window" />);
    expect(screen.getByTestId("gate-frame").classList.contains("onb-gate--full-window")).toBe(true);
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

  it("keeps draft-stop fail-safe, analytics, connection, and connect skip", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") return state("connect", 8);
      if (command === "permission_status" || command === "permission_listener_ready") return emptyPermissions;
      if (command === "composio_settings") return { draft_stop: true, consent_acknowledged: false };
      if (command === "set_composio_policy") throw new Error("consent required");
      if (command === "analytics_get_opt_out") return false;
      if (command === "connectors_list") return [];
      if (command === "set_onboarding_state") return state("gate", 9);
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Onboarding />);
    const toggle = await screen.findByRole("checkbox", { name: /Drafts only/ });
    fireEvent.click(toggle);
    expect(await screen.findByText("Turning this off needs your consent first — that lives in Settings.")).toBeTruthy();
    expect(screen.getByText("Share anonymous usage metrics to help improve SHOGUN")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Skip for now" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_onboarding_state", { expectedRevision: 8, step: "gate", plan: null, completed: false }));
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

  it("keeps reduced-motion and offscreen haze stop paths in scoped stylesheet", () => {
    const css = readFileSync(resolve(process.cwd(), "src/onboarding/onboarding.css"), "utf8");
    expect(css.includes("data-haze-motion=\"true\"")).toBe(true);
    expect(css.includes("prefers-reduced-motion: reduce")).toBe(true);
    expect(css.includes(".onb-cinematic__wave, .onb-ambient img, .onb-haze { animation: none;")).toBe(true);
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
