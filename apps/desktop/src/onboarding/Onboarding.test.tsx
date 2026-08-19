import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Onboarding } from "./Onboarding";

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("unified permission onboarding", () => {
  it("hydrates all live permissions instead of waiting for an event emitted before WebKit subscribed", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") {
        return {
          completed: true,
          step: "permission",
          plan: null,
          permissions_repair: true,
        };
      }
      if (command === "permission_status" || command === "permission_listener_ready") {
        return {
          accessibility: true,
          microphone: true,
          screen_recording: true,
          all_granted: true,
        };
      }
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Onboarding />);

    expect(await screen.findByText("3 of 3 ready")).toBeTruthy();
    expect(screen.getAllByText("Ready")).toHaveLength(3);
    expect(screen.queryByText("Waiting for permission…")).toBeNull();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("permission_listener_ready"));
  });

  it("shows every required permission together and blocks continue until all are ready", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") {
        return { completed: false, step: "permission", plan: null };
      }
      if (command === "permission_status") {
        return {
          accessibility: true,
          microphone: false,
          screen_recording: false,
          all_granted: false,
        };
      }
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Onboarding />);

    expect(await screen.findByText("Accessibility")).toBeTruthy();
    expect(screen.getByText("Microphone")).toBeTruthy();
    expect(screen.getByText("Screen Recording")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Continue" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Skip for now" })).toBeNull();
  });

  it("passes revision and stays on current step when Rust rejects a stale write", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") {
        return { completed: false, step: "welcome", plan: null, revision: 7 };
      }
      if (command === "permission_status") return { ...EMPTY_PERMISSION_RESULT };
      if (command === "onboarding_event") return undefined;
      if (command === "set_onboarding_state") throw new Error("stale onboarding revision");
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Onboarding />);
    fireEvent.click(await screen.findByRole("button", { name: "Set up SHOGUN" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_onboarding_state", {
        expectedRevision: 7,
        step: "reads",
        plan: null,
        completed: false,
      });
    });
    expect(screen.getByText("SHOGUN lives in the notch.")).toBeTruthy();
  });

  it("does not downgrade or overwrite a future semantic step", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") {
        return { completed: false, step: "screen_recording", plan: null, revision: 9 };
      }
      if (command === "permission_status") return { ...EMPTY_PERMISSION_RESULT };
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Onboarding />);

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("onboarding_state"));
    expect(screen.queryByRole("button")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("set_onboarding_state", expect.anything());
  });
});

const EMPTY_PERMISSION_RESULT = {
  accessibility: false,
  microphone: false,
  screen_recording: false,
  all_granted: false,
  accessibility_state: "not_granted",
  microphone_state: "not_determined",
  screen_recording_state: "not_granted",
  all_effective: false,
  reason: null,
  revision: 1,
};
