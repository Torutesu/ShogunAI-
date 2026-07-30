//! push-to-talk の計測（Issue #44）。
//!
//! Issue #44 は定量目標を持つ: アクティブユーザーの30%が週次でPTTを使うこと、セッションの
//! 半分が「完了した仕事」になること。その材料になるイベントが3つ — 開始 / 完了 / 失敗 —
//! だけをここに置く。
//!
//! **不変条件（CLAUDE.md）: テレメトリに発話・文字起こし・応答を載せない。** PTTを通る
//! ものは全て利用者の音声とそれへの回答なので、ここが運ぶのは時間（ms）と結果コードだけ。
//! 文字数すら送らない — 長さが内容の指紋になり得るなら、それも情報だから。時間と enum
//! コードがあれば「速いか」「失敗していないか」には十分答えられる。
//!
//! 現状の出力先は構造化ログ行（`eprintln!`）。これはこのcrateの既存の計測イディオムに
//! 揃えたもので、[`crate::onboarding`] の `onboarding_event` と `metrics` の SLO 記録が同じ
//! 形をとる — 「計測はデバイスから出ない（不変条件3）」を守りつつ、dev/internal ビルドの
//! ログからファネルを再構成できる。実際のPostHogシンクへの接続は別タスク（本crateには
//! まだPostHog連携が無い。task-lead へ報告済み）。

// Task 12 で `ptt.rs` から配線されるまでは未参照のモジュールなので、`pub` な関数も dead-code
// 判定を素通りしない。`ptt_lane` / `hold_monitor` / `notch_actions` / `approvals` と同じ idiom。
// 配線後は外せる。
#![allow(dead_code)]

/// push-to-talk のセッションが始まった（マイクが開いた）。
///
/// ここから下の3つが Issue #44 の定量目標の材料になる。**発話も文字起こしも応答も
/// 送らない** — 送るのは時間と結果コードだけで、それで「速いか」「失敗していないか」は
/// 十分に分かる。
pub fn capture_ptt_started(_app: &tauri::AppHandle) {
    eprintln!("[analytics] ptt_session_started");
}

/// 応答が最後まで届いた。`first_token_ms` が SLO-03（初トークン1s）の実測値。
pub fn capture_ptt_completed(_app: &tauri::AppHandle, first_token_ms: u64, total_ms: u64) {
    eprintln!("[analytics] ptt_session_completed first_token_ms={first_token_ms} total_ms={total_ms}");
}

/// セッションが失敗して終わった。`code` は `shogun_core::ptt::statemachine::Fail::code()`
/// が返す安定した文字列（`mic_unavailable` / `no_asr_model` / `nothing_heard` /
/// `asr_failed` / `network` / `key_rejected`）。
pub fn capture_ptt_failed(_app: &tauri::AppHandle, code: &str) {
    eprintln!("[analytics] ptt_session_failed code={code}");
}
