import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
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

  it("keeps gate frame mounted and leaves full-window API unreachable", () => {
    const { rerender } = render(<GateFrame />);
    const gate = screen.getByTestId("gate-frame");
    rerender(<GateFrame complete />);
    expect(screen.getByTestId("gate-frame")).toBe(gate);
    expect(gate.getAttribute("data-complete")).toBe("true");
    expect(gate.classList.contains("onb-gate--frame")).toBe(true);
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
  });
});
