import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./strings";

/** Anonymous-usage toggle (opt-out model — on by default). Copy lives in strings.ts. */
export function AnalyticsToggle() {
  const [optOut, setOptOut] = useState(false);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    invoke<boolean>("analytics_get_opt_out")
      .then((v) => setOptOut(v))
      .catch(() => setOptOut(false))
      .finally(() => setReady(true));
  }, []);

  async function toggle() {
    const next = !optOut;
    setOptOut(next);
    try {
      await invoke("analytics_set_opt_out", { optOut: next });
    } catch {
      setOptOut(!next); // revert on failure
    }
  }

  if (!ready) return null;

  return (
    <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <input type="checkbox" checked={!optOut} onChange={toggle} />
      <span>
        {t.analyticsToggleLabel}
        <br />
        <small>{t.analyticsToggleDetail}</small>
      </span>
    </label>
  );
}
