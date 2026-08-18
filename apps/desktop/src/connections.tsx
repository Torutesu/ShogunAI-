// First-layer connections (§6.9). One list, used by Settings and by onboarding — the row is where
// a service's whole state is expressed, so it must not exist twice and drift (the reason this
// moved out of App.tsx when onboarding needed the same row).
//
// Talks to the Rust connector commands (connectors_list / connect_service / disconnect_service);
// the data layer stays in Rust (invariant 1). This file is presentation only.

import { useCallback, useEffect, useState } from "react";
import type { JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SERVICE_ICONS } from "./serviceIcons";
import { t } from "./strings";

const IN_TAURI =
  typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

export type ConnState = "connected" | "needs_reauth" | "disconnected" | "coming_soon";

export interface ServiceStatus {
  source: string; // "gmail" | "gcal" | "gdrive" | "slack" | "notion" | "github" | "linear"
  state: ConnState;
  last_sync_ms: number | null;
  has_endpoint: boolean;
}

export const CONN_LABELS: Record<string, string> = {
  gmail: "Gmail",
  gcal: "Google Calendar",
  gdrive: "Google Drive",
  slack: "Slack",
  notion: "Notion",
  github: "GitHub",
  linear: "Linear",
};

// From the catalog, not literals: user-facing copy lives in strings.ts (i18n-ready, コード規約).
export const CONN_STATE_LABEL: Record<ConnState, string> = {
  connected: t.connStateConnected,
  needs_reauth: t.connStateNeedsReauth,
  disconnected: t.connStateDisconnected,
  coming_soon: t.connStateComingSoon,
};

// Brand marks, inlined from simple-icons at build time (see scripts/generate-service-icons.mjs).
// Services that project has removed on trademark request — Slack, OpenAI — fall back to a lettered
// disc in the service's own colour rather than an approximated logo.
const CONN_FALLBACK_TINT: Record<string, string> = {
  slack: "#611f69",
  openai: "#74aa9c",
};

/// Perceived luminance of a #rrggbb colour, 0..1 (Rec. 709 coefficients).
function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  return (0.2126 * r + 0.7152 * g + 0.0587 * b) / 255;
}

/// A service's mark: the real logo where we have one, a lettered disc where we don't.
///
/// Brand colours are used as-is except at the extremes — Notion and GitHub are near-black, which
/// disappears on the dark panel — where the mark falls back to the foreground colour so it stays
/// legible in whichever theme is showing.
export function ServiceMark(props: { source: string; label: string }): JSX.Element {
  const icon = SERVICE_ICONS[props.source];
  const raw = icon?.hex ?? CONN_FALLBACK_TINT[props.source] ?? "";
  const lum = raw ? luminance(raw) : 0.5;
  const tint = !raw || lum < 0.16 || lum > 0.9 ? "var(--ink)" : raw;
  return (
    <span className="conn__mark" style={{ "--tint": tint } as React.CSSProperties} aria-hidden="true">
      {icon ? (
        <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor" role="presentation">
          <path d={icon.path} />
        </svg>
      ) : (
        props.label.charAt(0)
      )}
    </span>
  );
}

/// The list of services and their state. `onChange` fires after every refresh (including the one
/// following a connect/disconnect), so a caller that reacts to "at least one connection" can do so
/// without polling.
export function ConnectionsList(props: {
  onChange?: (rows: ServiceStatus[]) => void;
  /** Drop the services that aren't built yet. Settings shows them (the roadmap is worth seeing);
   *  onboarding doesn't — a screen you're trying to finish is no place to list what can't be done. */
  connectableOnly?: boolean;
}): JSX.Element {
  const { onChange, connectableOnly } = props;
  const [rows, setRows] = useState<ServiceStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<ServiceStatus[]>("connectors_list")
      .then((r) => {
        setRows(r);
        setError(null);
        onChange?.(r);
      })
      .catch((e) => setError(String(e)));
  }, [onChange]);
  useEffect(refresh, [refresh]);

  const act = useCallback(
    (cmd: "connect_service" | "disconnect_service", source: string): void => {
      setBusy(source);
      setError(null);
      void invoke(cmd, { service: source })
        .then(refresh)
        .catch((e) => setError(String(e)))
        .finally(() => setBusy(null));
    },
    [refresh],
  );

  const shown = connectableOnly
    ? rows.filter((r) => r.has_endpoint && r.state !== "coming_soon")
    : rows;

  return (
    <>
      {error ? <div className="set__hint is-err">{error}</div> : null}
      {shown.length === 0 ? (
        <div className="set__hint">{t.connectionsEmpty}</div>
      ) : (
        <div className="conns">
          {shown.map((r) => {
            const label = CONN_LABELS[r.source] ?? r.source;
            const canConnect = r.has_endpoint && r.state !== "coming_soon";
            const connected = r.state === "connected" || r.state === "needs_reauth";
            const stateMod =
              r.state === "connected" ? " is-ok" : r.state === "needs_reauth" ? " is-warn" : "";
            return (
              <div key={r.source} className="conn">
                <ServiceMark source={r.source} label={label} />
                <div className="conn__meta">
                  <span className="conn__name">{label}</span>
                  <span className={`conn__state${stateMod}`}>
                    {CONN_STATE_LABEL[r.state]}
                    {r.last_sync_ms ? ` · ${new Date(r.last_sync_ms).toLocaleTimeString()}` : ""}
                  </span>
                </div>
                {r.state === "needs_reauth" ? (
                  // Amber (FR-INT-06): the retry affordance — re-run the connect flow in place.
                  <button
                    className="keyrow__btn"
                    type="button"
                    disabled={busy === r.source}
                    onClick={() => act("connect_service", r.source)}
                  >
                    {busy === r.source ? t.connecting : t.reconnect}
                  </button>
                ) : null}
                {connected ? (
                  <button
                    className="keyrow__btn"
                    type="button"
                    disabled={busy === r.source}
                    onClick={() => act("disconnect_service", r.source)}
                  >
                    {busy === r.source ? "…" : t.disconnect}
                  </button>
                ) : (
                  <button
                    className="keyrow__btn"
                    type="button"
                    disabled={!canConnect || busy === r.source}
                    onClick={() => act("connect_service", r.source)}
                    title={canConnect ? "" : t.connectionsUnavailable}
                  >
                    {busy === r.source ? t.connecting : t.connect}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
