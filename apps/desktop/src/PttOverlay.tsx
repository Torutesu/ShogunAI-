// push-to-talk のパネル（Issue #44）。
//
// 録音中・解析中・応答・エラーの4つを同じ位置・同じ枠で描き分ける。位置を変えないのは、
// 話し終えたユーザーの視線が既にそこにあるから。
//
// データは全て Rust から来る（不変条件1: webview にデータ層のロジックを置かない）。
// このファイルが持っているのは描画と、閉じる・コピーの操作だけ。

import { useEffect, useState, type JSX } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type PanelView =
  | { kind: "listening" }
  | { kind: "transcribing" }
  | { kind: "responding" }
  | { kind: "error"; code: string };

// 失敗理由の文言は Rust 側（ptt::fail_message）が持つ。ここでは受け取って出すだけ。
type ErrorPayload = { code: string; message: string };

export function PttOverlay(): JSX.Element | null {
  const [view, setView] = useState<PanelView | null>(null);
  const [answer, setAnswer] = useState("");
  const [errorText, setErrorText] = useState("");

  useEffect(() => {
    const unlisten = [
      listen<PanelView>("ptt:panel", (e) => {
        setView(e.payload);
        // 新しいセッションが始まったら前の応答を消す。前の答えの上に次の答えが
        // 積み上がると、どこまでが今の返事か分からなくなる。
        if (e.payload.kind === "listening") {
          setAnswer("");
          setErrorText("");
        }
      }),
      // 応答は届いた端から追記する。完成を待って一度に出すと、ストリーミングにした
      // 意味が消える。
      listen<string>("ptt:delta", (e) => setAnswer((a) => a + e.payload)),
      listen<ErrorPayload>("ptt:error", (e) => setErrorText(e.payload.message)),
    ];
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
    };
  }, []);

  // Esc で閉じる。録音中なら録音ごと捨てる（判断は Rust 側の状態機械が持つ）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void invoke("ptt_cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!view) return null;

  return (
    <div className="ptt-panel">
      {view.kind === "listening" && (
        <div className="ptt-row">
          <span className="ptt-mic ptt-mic--live" aria-hidden />
          <span className="ptt-label">Listening…</span>
          <span className="ptt-hint">Esc to cancel</span>
        </div>
      )}

      {view.kind === "transcribing" && (
        <div className="ptt-row">
          <span className="ptt-mic" aria-hidden />
          <span className="ptt-label">Working…</span>
        </div>
      )}

      {view.kind === "responding" && (
        <div className="ptt-answer">
          <p className="ptt-text">{answer}</p>
          <div className="ptt-actions">
            <button type="button" onClick={() => void navigator.clipboard.writeText(answer)}>Copy</button>
            <button type="button" onClick={() => void invoke("ptt_open_full_ui")}>Open in SHOGUN</button>
            <button type="button" onClick={() => void invoke("ptt_dismiss")}>Close</button>
          </div>
        </div>
      )}

      {view.kind === "error" && (
        <div className="ptt-answer ptt-answer--error">
          <p className="ptt-text">{errorText}</p>
          <div className="ptt-actions">
            {view.code === "mic_unavailable" && (
              <button type="button" onClick={() => void invoke("ptt_open_privacy_settings")}>
                Open Settings
              </button>
            )}
            <button type="button" onClick={() => void invoke("ptt_dismiss")}>Close</button>
          </div>
        </div>
      )}
    </div>
  );
}
