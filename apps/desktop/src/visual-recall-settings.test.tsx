import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { formatStorageBytes, VisualRecallSection } from "./App";

type TestStatus = {
  enabled: boolean;
  retention_days: number;
  events_24h: number;
  frames_count: number;
  frames_bytes: number;
  estimated_daily_bytes: number | null;
  projected_retention_bytes: number | null;
  capture_paused_storage: boolean;
  capture_storage_limit_bytes: number;
  recent: never[];
};

const baseStatus: TestStatus = {
  enabled: true,
  retention_days: 3,
  events_24h: 2,
  frames_count: 2,
  frames_bytes: 1024,
  estimated_daily_bytes: 2048,
  projected_retention_bytes: 6144,
  capture_paused_storage: false,
  capture_storage_limit_bytes: 2 * 1024 * 1024 * 1024,
  recent: [],
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(cleanup);

describe("Visual Recall retention storage display", () => {
  it("formats aggregate encrypted storage without exposing frame content", () => {
    expect(formatStorageBytes(0)).toBe("0 B");
    expect(formatStorageBytes(1_536)).toBe("1.5 KB");
    expect(formatStorageBytes(2 * 1024 * 1024 * 1024)).toBe("2.0 GB");
  });

  it("handles invalid estimates safely", () => {
    expect(formatStorageBytes(Number.NaN)).toBe("0 B");
    expect(formatStorageBytes(-1)).toBe("0 B");
  });
});

describe("VisualRecallSection retention behavior", () => {
  function mockInitial(days = 3, status: TestStatus = baseStatus): void {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_visual_recall_settings") {
        return { enabled: true, retention: { days } };
      }
      if (cmd === "get_visual_recall_status") return { ...status, retention_days: days };
      if (cmd === "set_visual_recall_retention") {
        return { enabled: true, retention: { days: (args as { days: number }).days } };
      }
      return undefined;
    });
  }

  it("restores persisted custom retention and shows pending estimate", async () => {
    mockInitial(30, {
      ...baseStatus,
      retention_days: 30,
      estimated_daily_bytes: null,
      projected_retention_bytes: null,
    });
    render(<VisualRecallSection />);

    expect(await screen.findByText("30 days")).toBeTruthy();
    expect(
      (screen.getByRole("spinbutton", { name: "Custom" }) as HTMLInputElement).valueAsNumber,
    ).toBe(30);
    expect(screen.getByText(/Storage estimate appears/)).toBeTruthy();
  });

  it("sends 1–7 slider values and renders the aggregate estimate", async () => {
    mockInitial();
    render(<VisualRecallSection />);
    await screen.findByText("3 days");

    fireEvent.change(screen.getByRole("slider", { name: "Keep saved screens" }), {
      target: { value: "5" },
    });
    await act(async () => undefined);

    expect(invoke).toHaveBeenCalledWith("set_visual_recall_retention", { days: 5 });
    expect(screen.getByText(/1.0 KB used now · about 6.0 KB/)).toBeTruthy();
  });

  it("validates custom days before invoking and applies a valid custom value", async () => {
    mockInitial();
    render(<VisualRecallSection />);
    await screen.findByText("3 days");
    fireEvent.click(screen.getByRole("button", { name: "Custom" }));

    const input = screen.getByRole("spinbutton", { name: "Custom" });
    fireEvent.change(input, { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(screen.getAllByText(/Choose 1–3,650 days/)).toHaveLength(2);
    expect(invoke).not.toHaveBeenCalledWith("set_visual_recall_retention", expect.anything());

    fireEvent.change(input, { target: { value: "14" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await act(async () => undefined);
    expect(invoke).toHaveBeenCalledWith("set_visual_recall_retention", { days: 14 });
  });

  it("rolls back retention when the backend rejects it", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_visual_recall_settings") {
        return { enabled: true, retention: { days: 3 } };
      }
      if (cmd === "get_visual_recall_status") return baseStatus;
      if (cmd === "set_visual_recall_retention") throw new Error("save failed");
      return undefined;
    });
    render(<VisualRecallSection />);
    await screen.findByText("3 days");
    fireEvent.change(screen.getByRole("slider", { name: "Keep saved screens" }), {
      target: { value: "6" },
    });
    await screen.findByText("Error: save failed");
    expect(screen.getByText("3 days")).toBeTruthy();
  });
});
