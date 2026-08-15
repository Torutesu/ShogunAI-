// Daily summaries (issue #10): the card renders what the Rust side composed, marks itself seen
// exactly once on open (既読 = カードを開いた), and each line's chip re-opens its data source.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SummaryCard, type MorningView, type WrapView } from "./daily";

const MORNING: MorningView = {
  generated: true,
  charm_line: "Two vendor calls today — your calm is the lever.",
  today: [
    { time: "09:30", title: "Weekly sync", updated: false },
    { time: "13:00", title: "Vendor renewal call", updated: true },
  ],
  commitments_due: [
    { text: "Send the Q3 deck to Alice", possibly: false, provenance_event_id: 41, source: "Mail" },
    { text: "Confirm the vendor contract", possibly: true, provenance_event_id: 42, source: "Notion" },
  ],
  open_loops: [
    { text: "Reply to Jordan — pricing thread", possibly: false, provenance_event_id: 43, source: "Slack" },
  ],
  what_happened: ["Shipped the report."],
};

const WRAP: WrapView = {
  outcome: { commitments_done: 3, loops_closed: 2, actions_decided: 5, actions_adopted: 4 },
  still_open: [
    { text: "Send the budget line to finance", possibly: false, provenance_event_id: 51, source: "Mail" },
  ],
  tomorrow_calendar: [{ time: "09:00", title: "Standup", updated: false }],
  tomorrow_commitments: [],
  loose_ends: [
    { text: "Chase the security questionnaire", possibly: true, provenance_event_id: 52, source: "Slack" },
  ],
};

async function mockInvoke(cards: { morning?: MorningView; evening?: WrapView }) {
  const { invoke } = await import("@tauri-apps/api/core");
  const calls: Array<{ cmd: string; args: unknown }> = [];
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    calls.push({ cmd, args });
    if (cmd === "mark_summary_seen") return undefined;
    if (cmd === "morning_card" && cards.morning) return cards.morning;
    if (cmd === "evening_wrap" && cards.evening) return cards.evening;
    if (cmd === "open_summary_source") return undefined;
    throw new Error(`unmocked: ${cmd}`);
  });
  return calls;
}

beforeEach(() => {
  vi.clearAllMocks();
});

// vitest runs without injected globals, so testing-library's auto-cleanup never hooks afterEach —
// without this, every render stacks and the queries start finding two of everything.
afterEach(cleanup);

describe("SummaryCard", () => {
  it("renders the morning greeting, charm line and sections from the wire payload", async () => {
    await mockInvoke({ morning: MORNING });
    render(<SummaryCard which="morning" date="2026-08-15" onClose={() => undefined} />);
    await act(async () => {});

    expect(screen.getByText("Good morning")).toBeTruthy();
    expect(screen.getByText(/your calm is the lever/)).toBeTruthy();
    expect(screen.getByText("Weekly sync")).toBeTruthy();
    expect(screen.getByText("Send the Q3 deck to Alice")).toBeTruthy();
    // FR-MB-05: the medium-confidence hedge survives to the pixels.
    expect(screen.getByText("possibly")).toBeTruthy();
    // FR-MB-06: a calendar line that changed carries its Updated mark.
    expect(screen.getByText("Updated")).toBeTruthy();
  });

  it("marks itself seen exactly once, with the delivery date it was opened for", async () => {
    const calls = await mockInvoke({ morning: MORNING });
    render(<SummaryCard which="morning" date="2026-08-15" onClose={() => undefined} />);
    await act(async () => {});

    const seen = calls.filter((c) => c.cmd === "mark_summary_seen");
    expect(seen).toHaveLength(1);
    expect(seen[0].args).toEqual({ which: "morning", date: "2026-08-15" });
  });

  it("opens a line's data source from its chip", async () => {
    const calls = await mockInvoke({ morning: MORNING });
    render(<SummaryCard which="morning" date="2026-08-15" onClose={() => undefined} />);
    await act(async () => {});

    fireEvent.click(screen.getByRole("button", { name: "Mail" }));
    const opened = calls.filter((c) => c.cmd === "open_summary_source");
    expect(opened).toHaveLength(1);
    expect(opened[0].args).toEqual({ eventId: 41 });
  });

  it("renders the evening outcome and still-open list, and Done dismisses", async () => {
    await mockInvoke({ evening: WRAP });
    const onClose = vi.fn();
    render(<SummaryCard which="evening" date="2026-08-15" onClose={onClose} />);
    await act(async () => {});

    expect(screen.getByText("Good evening")).toBeTruthy();
    expect(screen.getByText("3")).toBeTruthy();
    expect(screen.getByText("4/5")).toBeTruthy();
    expect(screen.getByText("Send the budget line to finance")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
