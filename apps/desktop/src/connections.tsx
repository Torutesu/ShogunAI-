// First-layer connections (§6.9). One list, used by Settings and by onboarding — the row is where
// a service's whole state is expressed, so it must not exist twice and drift.
//
// Talks to the Rust connector commands (connectors_list / connect_service / disconnect_service);
// the data layer stays in Rust (invariant 1). This file is presentation only.

import { useCallback, useEffect, useState } from "react";
import type { JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IN_TAURI } from "./tauri";
import { SERVICE_ICONS } from "./serviceIcons";
import { Icon } from "./icons";
import { t } from "./strings";

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

const CONN_STATE_LABEL: Record<ConnState, string> = {
  connected: "Connected",
  needs_reauth: "Needs reauth",
  disconnected: "Not connected",
  coming_soon: "Coming soon",
};

// Brand marks are inlined from simple-icons at build time (scripts/generate-service-icons.mjs).
// Services that project has removed on trademark request — Slack, OpenAI — fall back to a lettered
// tile in the service's own colour rather than an approximated logo.
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

/// A service's mark on a white tile — a logo belongs on its own ground, so the tile stays white on
/// both themes and only a mark that would vanish against white is re-tinted.
export function ServiceMark(props: { source: string; label: string }): JSX.Element {
  const icon = SERVICE_ICONS[props.source];
  const raw = icon?.hex ?? CONN_FALLBACK_TINT[props.source] ?? "";
  const lum = raw ? luminance(raw) : 0.5;
  const tint = !raw || lum > 0.9 ? "#1d1d1f" : raw;
  return (
    <span className="conn__mark" style={{ "--tint": tint } as React.CSSProperties} aria-hidden="true">
      {icon ? (
        <svg viewBox="0 0 24 24" width="19" height="19" fill="currentColor" role="presentation">
          <path d={icon.path} />
        </svg>
      ) : (
        props.label.charAt(0)
      )}
    </span>
  );
}

/// The list of services and their state. `onChange` fires after a connect/disconnect settles, so
/// a caller that gates on "at least one connection" can react without polling.
export function ConnectionsList(props: {
  onChange?: (rows: ServiceStatus[]) => void;
  /** Drop the services that aren't built yet. Settings shows them (the roadmap is worth seeing);
   *  onboarding does not, because a step you are trying to finish should only list things you can
   *  actually do. */
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

  if (error) return <div className="set__hint is-err">{error}</div>;
  if (rows.length === 0) return <div className="set__hint">{t.connectionsEmpty}</div>;

  const shown = connectableOnly ? rows.filter((r) => r.state !== "coming_soon") : rows;

  return (
    <div className="conns">
      {shown.map((r) => {
        const label = CONN_LABELS[r.source] ?? r.source;
        const canConnect = r.has_endpoint && r.state !== "coming_soon";
        const connected = r.state === "connected" || r.state === "needs_reauth";
        const stateMod =
          r.state === "connected" ? " is-ok" : r.state === "needs_reauth" ? " is-warn" : "";
        // A disconnected service says what it would DO for you; a connected one says where it
        // stands. The second line is never blank, and never repeats the name.
        const line = connected
          ? `${CONN_STATE_LABEL[r.state]}${
              r.last_sync_ms ? ` · ${new Date(r.last_sync_ms).toLocaleTimeString()}` : ""
            }`
          : r.state === "coming_soon"
            ? t.connectionsUnavailable
            : (t.connectionBlurbs[r.source] ?? CONN_STATE_LABEL[r.state]);
        return (
          <div key={r.source} className={`conn${r.state === "coming_soon" ? " is-soon" : ""}`}>
            <ServiceMark source={r.source} label={label} />
            <div className="conn__meta">
              <span className="conn__name">{label}</span>
              <span className={`conn__state${connected ? stateMod : ""}`}>{line}</span>
            </div>
            {/* One control per row, and it says what the row needs: a tick you can click to
                disconnect, a warning you can click to sign in again, a plus to connect. A service
                that isn't available yet gets no control at all — a permanently disabled button is
                just noise. */}
            {r.state === "connected" ? (
              <button
                className="conn__act conn__act--on"
                type="button"
                title={t.disconnect}
                aria-label={`${t.disconnect} ${label}`}
                disabled={busy === r.source}
                onClick={() => act("disconnect_service", r.source)}
              >
                <Icon name="check" size={16} />
              </button>
            ) : r.state === "needs_reauth" ? (
              <button
                className="conn__act conn__act--warn"
                type="button"
                title={t.reconnect}
                aria-label={`${t.reconnect} ${label}`}
                disabled={busy === r.source}
                onClick={() => act("connect_service", r.source)}
              >
                <Icon name="arrow" size={16} />
              </button>
            ) : canConnect ? (
              <button
                className="conn__act"
                type="button"
                title={t.connect}
                aria-label={`${t.connect} ${label}`}
                disabled={busy === r.source}
                onClick={() => act("connect_service", r.source)}
              >
                <Icon name="plus" size={16} />
              </button>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
