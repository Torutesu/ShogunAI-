// First-layer connections screen (§6.9). ROUGH — styling is a later pass; this exists so the
// connect / disconnect / status flow is usable end to end. Talks to the Rust connector commands
// (connectors_list / connect_service / disconnect_service). Mount it in the Full UI / Settings
// window (the notch panel stays as-is).
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type ConnState = "connected" | "needs_reauth" | "disconnected" | "coming_soon";

interface ServiceStatus {
  source: string; // "gmail" | "gcal" | "gdrive" | ...
  state: ConnState;
  last_sync_ms: number | null;
  has_endpoint: boolean;
}

const LABELS: Record<string, string> = {
  gmail: "Gmail",
  gcal: "Google Calendar",
  gdrive: "Google Drive",
  slack: "Slack",
  notion: "Notion",
  github: "GitHub",
  linear: "Linear",
};

const STATE_LABEL: Record<ConnState, string> = {
  connected: "Connected",
  needs_reauth: "Needs reauth",
  disconnected: "Not connected",
  coming_soon: "Coming soon",
};

export function Connections(): JSX.Element {
  const [rows, setRows] = useState<ServiceStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    void invoke<ServiceStatus[]>("connectors_list")
      .then(setRows)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const onConnect = useCallback(
    (source: string) => {
      setBusy(source);
      setError(null);
      void invoke("connect_service", { service: source })
        .then(refresh)
        .catch((e) => setError(String(e)))
        .finally(() => setBusy(null));
    },
    [refresh],
  );

  const onDisconnect = useCallback(
    (source: string) => {
      setBusy(source);
      setError(null);
      void invoke("disconnect_service", { service: source })
        .then(refresh)
        .catch((e) => setError(String(e)))
        .finally(() => setBusy(null));
    },
    [refresh],
  );

  return (
    <div style={{ padding: 16, fontFamily: "system-ui", maxWidth: 520 }}>
      <h2 style={{ marginBottom: 4 }}>Connections</h2>
      <p style={{ opacity: 0.6, marginTop: 0, fontSize: 13 }}>
        First-layer integrations connect directly to each service. Data stays on your device.
      </p>
      {error && (
        <div style={{ color: "#b00", fontSize: 13, margin: "8px 0" }}>{error}</div>
      )}
      <ul style={{ listStyle: "none", padding: 0 }}>
        {rows.map((r) => {
          const label = LABELS[r.source] ?? r.source;
          const canConnect = r.has_endpoint && r.state !== "coming_soon";
          const connected = r.state === "connected" || r.state === "needs_reauth";
          return (
            <li
              key={r.source}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "10px 0",
                borderBottom: "1px solid rgba(0,0,0,0.08)",
              }}
            >
              <div>
                <div style={{ fontWeight: 600 }}>{label}</div>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  {STATE_LABEL[r.state]}
                  {r.last_sync_ms
                    ? ` · last sync ${new Date(r.last_sync_ms).toLocaleTimeString()}`
                    : ""}
                </div>
              </div>
              {connected ? (
                <button disabled={busy === r.source} onClick={() => onDisconnect(r.source)}>
                  Disconnect
                </button>
              ) : (
                <button
                  disabled={!canConnect || busy === r.source}
                  onClick={() => onConnect(r.source)}
                  title={canConnect ? "" : "Not available yet"}
                >
                  {busy === r.source ? "Connecting…" : "Connect"}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
