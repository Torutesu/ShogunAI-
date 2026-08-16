// Issue #82 §4-2/4-3, at the row the user actually reads. Three things are pinned here because
// each one fails silently if it regresses: the access badge must not overstate what a connection
// grants, amber must read as resumable rather than broken, and Disconnect must not delete a
// Keychain token on one stray click.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ConnectionsList, relativeTime, stateLine, type ServiceStatus } from "./connections";
import { t } from "./strings";

function row(over: Partial<ServiceStatus> = {}): ServiceStatus {
  return {
    source: "gcal",
    state: "connected",
    last_sync_ms: null,
    has_endpoint: true,
    access: "read_write",
    ...over,
  };
}

/** Serve `connectors_list` from `rows`, recording every other command the row fires. */
async function mockConnectors(rows: ServiceStatus[]) {
  const calls: { cmd: string; service: string }[] = [];
  const { invoke } = await import("@tauri-apps/api/core");
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "connectors_list") return rows;
    calls.push({ cmd, service: (args as { service: string }).service });
    return undefined;
  });
  return calls;
}

async function renderList(rows: ServiceStatus[]) {
  const calls = await mockConnectors(rows);
  render(<ConnectionsList />);
  await waitFor(() => expect(screen.getByText(/Google Calendar|Gmail/)).toBeTruthy());
  return calls;
}

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("the access badge", () => {
  it("says Read & Draft for a connection that cannot send", async () => {
    // Gmail's first-layer OAuth carries no send scope — a "Read & Write" badge here would promise
    // the user something the connection cannot do.
    await renderList([row({ source: "gmail", access: "read_draft" })]);
    expect(screen.getByText(t.connAccess.read_draft)).toBeTruthy();
    expect(screen.queryByText(t.connAccess.read_write)).toBeNull();
  });

  it("falls back to the raw id rather than rendering nothing for an unknown range", async () => {
    await renderList([row({ access: "read_someday" })]);
    expect(screen.getByText("read_someday")).toBeTruthy();
  });
});

describe("the third-party disclosure", () => {
  it("marks the Gmail row as routed through Composio", async () => {
    await renderList([row({ source: "gmail", access: "read_draft" })]);
    expect(screen.getByText(t.connViaComposio)).toBeTruthy();
  });

  it("leaves a direct connection unmarked", async () => {
    await renderList([row()]);
    expect(screen.queryByText(t.connViaComposio)).toBeNull();
  });
});

describe("the state line", () => {
  it("tells an amber service to reconnect instead of naming a failure", () => {
    expect(stateLine(row({ state: "needs_reauth" }))).toBe(t.connExpired);
  });

  it("distinguishes connected-but-never-synced from a sync that happened", () => {
    // A restored connection has no last-sync time this process. Showing a fabricated one — or the
    // epoch — would misreport freshness, so it says it is waiting.
    expect(stateLine(row({ last_sync_ms: null }))).toBe(t.connNeverSynced);
    const now = 1_700_000_000_000;
    expect(stateLine(row({ last_sync_ms: now - 5 * 60_000 }), now)).toBe(
      t.connLastSync.replace("{ago}", relativeTime(now - 5 * 60_000, now)),
    );
  });

  it("counts elapsed time in the largest unit that fits", () => {
    const now = 1_700_000_000_000;
    expect(relativeTime(now - 30_000, now)).toBe(relativeTime(now, now));
    expect(relativeTime(now - 5 * 60_000, now)).toMatch(/5 minutes ago/);
    expect(relativeTime(now - 3 * 3_600_000, now)).toMatch(/3 hours ago/);
    expect(relativeTime(now - 2 * 86_400_000, now)).toMatch(/2 days ago/);
  });
});

describe("disconnect", () => {
  it("asks before deleting the token, and does nothing until confirmed", async () => {
    const calls = await renderList([row()]);

    await act(async () => screen.getByRole("button", { name: t.disconnect }).click());
    expect(
      screen.getByText(t.connDisconnectConfirm.replace("{service}", "Google Calendar")),
    ).toBeTruthy();
    expect(calls).toHaveLength(0);

    await act(async () => screen.getByRole("button", { name: t.connDisconnectCancel }).click());
    expect(calls).toHaveLength(0);
    expect(screen.queryByText(/Disconnect Google Calendar\?/)).toBeNull();
  });

  it("disconnects once confirmed", async () => {
    const calls = await renderList([row()]);
    await act(async () => screen.getByRole("button", { name: t.disconnect }).click());
    // Two buttons now read "Disconnect" — the row's and the confirmation's. The one inside the
    // alertdialog is the one that acts.
    const dialog = screen.getByRole("alertdialog");
    const confirm = Array.from(dialog.querySelectorAll("button")).find(
      (b) => b.textContent === t.disconnect,
    );
    await act(async () => confirm?.click());

    expect(calls).toEqual([{ cmd: "disconnect_service", service: "gcal" }]);
  });
});
