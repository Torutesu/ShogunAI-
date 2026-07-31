import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * 「匿名の利用状況を送信」トグル（オプトアウト方式・既定ON）。
 */
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
      setOptOut(!next); // 失敗したら戻す
    }
  }

  if (!ready) return null;

  return (
    <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <input type="checkbox" checked={!optOut} onChange={toggle} />
      <span>
        匿名の利用状況を送信して改善に協力する
        <br />
        <small>個人データ・画面キャプチャ内容・APIキーは一切送りません。</small>
      </span>
    </label>
  );
}
