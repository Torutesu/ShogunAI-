import { cleanup, render, screen } from "@testing-library/react";
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
      if (command === "permission_status") {
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
});
