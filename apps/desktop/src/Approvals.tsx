// L3 confirmation queue (§6.6 / FR-AG-03). ROUGH — styling later. Shows each pending send's full
// content (never a summary) and a dedicated Confirm button (Enter alone must not confirm). Talks to
// the Rust approval commands (list_approvals / confirm_send / reject_send).
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ApprovalView {
  id: number;
  op_type: string;
  destination: string;
  full_body: string;
  route: string; // "direct" | "composio"
}

export function Approvals(): JSX.Element {
  const [rows, setRows] = useState<ApprovalView[]>([]);
  const [busy, setBusy] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    void invoke<ApprovalView[]>("list_approvals")
      .then(setRows)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [refresh]);

  const act = useCallback(
    (cmd: "confirm_send" | "reject_send", id: number) => {
      setBusy(id);
      setError(null);
      void invoke<string>(cmd, { id })
        .then((outcome) => {
          if (outcome.startsWith("failed:")) setError(outcome);
        })
        .catch((e) => setError(String(e)))
        .finally(() => {
          setBusy(null);
          refresh();
        });
    },
    [refresh],
  );

  return (
    <div style={{ padding: 16, fontFamily: "system-ui", maxWidth: 520 }}>
      <h2 style={{ marginBottom: 4 }}>Approvals</h2>
      <p style={{ opacity: 0.6, marginTop: 0, fontSize: 13 }}>
        Actions that leave your device wait here for explicit confirmation.
      </p>
      {error && <div style={{ color: "#b00", fontSize: 13, margin: "8px 0" }}>{error}</div>}
      {rows.length === 0 && <div style={{ opacity: 0.5, fontSize: 13 }}>Nothing to confirm.</div>}
      <ul style={{ listStyle: "none", padding: 0 }}>
        {rows.map((r) => (
          <li key={r.id} style={{ padding: "12px 0", borderBottom: "1px solid rgba(0,0,0,0.08)" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
              <strong>{r.op_type}</strong>
              <span style={{ fontSize: 11, opacity: 0.6 }}>
                {r.destination} · {r.route === "composio" ? "third-party (Composio)" : "direct"}
              </span>
            </div>
            <pre
              style={{
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
                background: "rgba(0,0,0,0.04)",
                padding: 8,
                borderRadius: 6,
                fontSize: 12,
                maxHeight: 200,
                overflow: "auto",
              }}
            >
              {r.full_body}
            </pre>
            <div style={{ display: "flex", gap: 8 }}>
              <button disabled={busy === r.id} onClick={() => act("confirm_send", r.id)}>
                {busy === r.id ? "…" : "Confirm & send"}
              </button>
              <button disabled={busy === r.id} onClick={() => act("reject_send", r.id)}>
                Reject
              </button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
