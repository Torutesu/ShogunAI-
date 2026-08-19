import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./strings";

/** Anonymous-usage toggle (opt-out model — on by default). Copy lives in strings.ts. */
export function AnalyticsToggle() {
  const [optOut, setOptOut] = useState(false);
  const [ready, setReady] = useState(false);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    invoke<boolean>("analytics_get_opt_out")
      .then((v) => setOptOut(v))
      .catch(() => setOptOut(false))
      .finally(() => setReady(true));
  }, []);

  async function toggle(enabled: boolean) {
    const nextOptOut = !enabled;
    setOptOut(nextOptOut);
    setPending(true);
    try {
      await invoke("analytics_set_opt_out", { optOut: nextOptOut });
    } catch {
      setOptOut(!nextOptOut); // revert on failure
    } finally {
      setPending(false);
    }
  }

  if (!ready) return null;

  return (
    <label className="analytics-toggle" style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
      <input type="checkbox" name="anonymous-usage-metrics" checked={!optOut} disabled={pending} onChange={(event) => void toggle(event.target.checked)} />
      <span>
        {t.analyticsToggleLabel}
        <br />
        <small>{t.analyticsToggleDetail}</small>
      </span>
    </label>
  );
}
