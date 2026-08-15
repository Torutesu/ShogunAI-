// Issue #121 at the last pixel: the search box must not draw "no matches" when the store failed.
// The two outcomes look identical in the old shape (an empty list), which is exactly the bug —
// so these pin them apart where the user actually reads them.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SearchBox } from "./App";
import { t } from "./strings";

const HIT = {
  event_id: 1,
  ts: 1_700_000_000_000,
  source: "ax",
  app: "com.apple.mail",
  excerpt: "Alice asked for the Q3 deck by Friday",
};

async function mockSearch(impl: (query: string) => Promise<unknown>) {
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "search_memory") return impl((args as { query: string }).query);
    // focus_field / record_ui_slo are fire-and-forget UI plumbing.
    return undefined;
  });
}

/** Type a query and let the 150ms debounce plus the promise settle. */
async function type(query: string) {
  fireEvent.change(screen.getByRole("textbox"), { target: { value: query } });
  await act(async () => {
    vi.advanceTimersByTime(200);
  });
  await act(async () => {});
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("SearchBox", () => {
  it("shows the empty-result copy when the store answers with no matches", async () => {
    await mockSearch(async () => []);
    render(<SearchBox onClose={() => undefined} />);
    await type("budget");

    expect(screen.getByText(t.searchEmpty)).toBeTruthy();
    expect(screen.queryByText(t.searchUnavailable)).toBeNull();
  });

  it("says memory is unavailable when the store fails, not that nothing matched", async () => {
    await mockSearch(async () => {
      throw new Error("Memory is unavailable right now (query)");
    });
    render(<SearchBox onClose={() => undefined} />);
    await type("budget");

    expect(screen.getByText(t.searchUnavailable)).toBeTruthy();
    expect(screen.queryByText(t.searchEmpty)).toBeNull();
  });

  it("clears the failure state once the store answers again", async () => {
    let fail = true;
    await mockSearch(async () => {
      if (fail) throw new Error("query");
      return [HIT];
    });
    render(<SearchBox onClose={() => undefined} />);
    await type("budget");
    expect(screen.getByText(t.searchUnavailable)).toBeTruthy();

    fail = false;
    await type("budget review");
    expect(screen.queryByText(t.searchUnavailable)).toBeNull();
    expect(screen.getByText(/Alice asked for the Q3 deck/)).toBeTruthy();
  });
});
