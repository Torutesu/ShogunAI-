import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Onboarding } from "./Onboarding";

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("Accessibility permission recovery", () => {
  it("hydrates live trust instead of waiting for an event emitted before WebKit subscribed", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "onboarding_state") {
        return {
          completed: true,
          step: "permission",
          plan: null,
          accessibility_repair: true,
        };
      }
      if (command === "accessibility_status") return true;
      if (command === "onboarding_event") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Onboarding />);

    expect(await screen.findByText("Granted — it's reading")).toBeTruthy();
    expect(screen.queryByText("Waiting for permission…")).toBeNull();
  });
});
