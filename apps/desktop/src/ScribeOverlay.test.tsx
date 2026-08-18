import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ScribeOverlay, sessionFromUrl } from "./ScribeOverlay";

interface TestEvent {
  payload: {
    session_id: number;
    phase: "opened" | "processing" | "inserted" | "failed" | "closed" | "cancelled" | "no_key";
    chars: number;
    detail: string | null;
  };
}

let deliver: ((event: TestEvent) => void) | undefined;

beforeEach(async () => {
  vi.clearAllMocks();
  window.history.replaceState({}, "", "/?session=42");
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    deliver = handler as unknown as (event: TestEvent) => void;
    return () => undefined;
  });
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "scribe_submit" || command === "scribe_close") return undefined;
    if (command === "scribe_status") {
      return { session_id: 42, phase: "processing", chars: 0, detail: null };
    }
    throw new Error(`unexpected command: ${command}`);
  });
});

afterEach(() => {
  cleanup();
  deliver = undefined;
});

describe("ScribeOverlay", () => {
  it("parses only a positive integer session", () => {
    expect(sessionFromUrl("?session=42")).toBe(42);
    expect(sessionFromUrl("?session=0")).toBeNull();
    expect(sessionFromUrl("?session=wat")).toBeNull();
  });

  it("ignores foreign events and restores the instruction after failure", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    render(<ScribeOverlay />);
    const input = screen.getByRole("textbox") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Make this clearer" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "scribe_submit")).toHaveLength(1);
    expect(input.disabled).toBe(true);
    await act(async () => {
      deliver?.({
        payload: { session_id: 99, phase: "failed", chars: 0, detail: "wrong session" },
      });
    });
    expect(input.disabled).toBe(true);

    await act(async () => {
      deliver?.({
        payload: { session_id: 42, phase: "failed", chars: 0, detail: "Try again" },
      });
    });
    expect(input.disabled).toBe(false);
    expect(input.value).toBe("Make this clearer");
    expect(input.placeholder).toBe("Try again");
  });

  it("blocks duplicate submit and closes the captured session on Escape", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    render(<ScribeOverlay />);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "Shorter" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "scribe_submit")).toHaveLength(1);

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("scribe_close", { sessionId: 42 });
    });
  });

  it("restores and refocuses the instruction when submit is rejected immediately", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "scribe_submit") throw new Error("busy");
      if (command === "scribe_status") {
        return { session_id: 42, phase: "processing", chars: 0, detail: null };
      }
      return undefined;
    });
    render(<ScribeOverlay />);
    const input = screen.getByRole("textbox") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Keep my words" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(input.value).toBe("Keep my words"));
    await waitFor(() => expect(document.activeElement).toBe(input));
  });
});
