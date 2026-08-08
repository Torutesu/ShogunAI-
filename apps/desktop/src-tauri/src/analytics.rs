//! Tauri シェル側の分析アダプタ。`analytics.json`（distinct_id + opt_out）の永続化、
//! distinct_id の生成、共通プロパティ組み立て、ハンドル生成、opt-out コマンドを担う。
//!
//! 送信ロジック本体は `shogun_core::analytics`。ここは OS/設定/配線だけ。

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use shogun_core::analytics::{AnalyticsConfig, AnalyticsHandle, Props};
use shogun_core::analytics::reqwest_transport::ReqwestTransport;

/// `analytics.json` の内容（非シークレット）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsState {
    /// 匿名の永続 distinct_id（UUIDv4 文字列）。
    pub distinct_id: String,
    /// テレメトリ送信を止めるか（既定 false = 送信ON）。
    #[serde(default)]
    pub opt_out: bool,
}

/// OS CSPRNG から UUIDv4 文字列を生成する（getrandom、シェルに既存の乱数源）。
pub fn new_distinct_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| format!("csprng failed: {e}"))?;
    // version 4 / variant 10xx
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

/// 呼び出し元（Tauri state）が持つ no-op 安全なラッパ。
/// `SHOGUN_POSTHOG_KEY` 未設定なら `None` で全 capture が no-op。
#[derive(Clone)]
pub struct Analytics(Option<AnalyticsHandle>);

impl Analytics {
    pub fn capture(&self, event: &str, props: Props) {
        if let Some(h) = &self.0 {
            h.capture(event, props);
        }
    }
    pub fn set_opt_out(&self, value: bool) {
        if let Some(h) = &self.0 {
            h.set_opt_out(value);
        }
    }
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("analytics.json"))
}

/// `analytics.json` を読む。無ければ distinct_id を採番して保存し返す。
fn load_or_init_state(app: &AppHandle) -> Result<AnalyticsState, String> {
    let path = state_path(app).ok_or("no app_data_dir")?;
    if let Ok(s) = std::fs::read_to_string(&path) {
        if let Ok(state) = serde_json::from_str::<AnalyticsState>(&s) {
            return Ok(state);
        }
    }
    let state = AnalyticsState { distinct_id: new_distinct_id()?, opt_out: false };
    save_state(app, &state);
    Ok(state)
}

/// `analytics.json` を書く（ベストエフォート、失敗は握りつぶす）。
pub fn save_state(app: &AppHandle, state: &AnalyticsState) {
    let Some(path) = state_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(state) {
        let _ = std::fs::write(&path, json);
    }
}

/// 全イベント共通のプロパティ。v1 は os / app_version / plan。
fn base_props(app: &AppHandle) -> Props {
    let mut p = Props::new();
    p.insert("os".into(), serde_json::Value::from(std::env::consts::OS));
    p.insert("app_version".into(), app.package_info().version.to_string().into());
    // v1 は課金基盤前のため "trial" 固定（fullui.rs と同じ実態）。
    p.insert("plan".into(), "trial".into());
    p
}

/// コネクタ read-sync 完了 → context_updated のプロパティに変換する純関数。
pub fn context_updated_props(source: &str, count: u64) -> Props {
    let mut p = Props::new();
    p.insert("source".into(), serde_json::Value::from(source));
    p.insert("newly_inserted".into(), serde_json::Value::from(count));
    p
}

/// 現在の opt_out を返す（フロントのトグル初期値）。
#[tauri::command]
pub fn analytics_get_opt_out(app: AppHandle) -> bool {
    match load_or_init_state(&app) {
        Ok(s) => s.opt_out,
        Err(_) => false,
    }
}

/// opt_out を設定：`analytics.json` を更新し、稼働中ハンドルにも即時反映。
#[tauri::command]
pub fn analytics_set_opt_out(
    app: AppHandle,
    analytics: tauri::State<'_, Analytics>,
    opt_out: bool,
) -> Result<(), String> {
    let mut state = load_or_init_state(&app)?;
    state.opt_out = opt_out;
    save_state(&app, &state);
    analytics.set_opt_out(opt_out);
    Ok(())
}

/// ビルド時に埋め込む PostHog project write key（配布ビルドの既定値）。
///
/// **リリース CI は `SHOGUN_POSTHOG_KEY` をビルド環境（CI secret）に設定すること** —
/// 未設定でビルドすると配布物は分析無効のまま出荷される（Issue #99 の元不具合）。
/// キーは write-only / 公開安全設計だがリポジトリにはコミットしない。
/// `build.rs` の `rerun-if-env-changed` が env 変更時の再コンパイルを保証する。
const BUILT_IN_POSTHOG_KEY: Option<&str> = option_env!("SHOGUN_POSTHOG_KEY");

/// 分析を初期化する。キー解決は 実行時 env（ローカル開発の上書き）→ ビルド時埋め込み → 無効。
/// キーが両方無ければ no-op ラッパを返す（開発ビルドで無害）。
pub fn init(app: &AppHandle) -> Analytics {
    let runtime_key = std::env::var("SHOGUN_POSTHOG_KEY").ok();
    let Some(key) =
        shogun_core::analytics::resolve_api_key(runtime_key.as_deref(), BUILT_IN_POSTHOG_KEY)
    else {
        eprintln!("[analytics] no PostHog key (runtime env or build-time embed) — analytics disabled");
        return Analytics(None);
    };
    let host = std::env::var("SHOGUN_POSTHOG_HOST")
        .unwrap_or_else(|_| "https://us.i.posthog.com".to_string());
    let state = match load_or_init_state(app) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[analytics] state init failed: {e} — analytics disabled");
            return Analytics(None);
        }
    };
    let Some(transport) = ReqwestTransport::new(&host) else {
        eprintln!("[analytics] TLS init failed — analytics disabled");
        return Analytics(None);
    };
    let config = AnalyticsConfig { api_key: key, distinct_id: state.distinct_id };
    let opt_out = Arc::new(AtomicBool::new(state.opt_out));
    let handle = AnalyticsHandle::spawn_shared(config, base_props(app), opt_out, transport);
    Analytics(Some(handle))
}

// --- push-to-talk の計測（Issue #44）---
//
// Issue #44 は定量目標を持つ: アクティブユーザーの30%が週次でPTTを使うこと、セッションの
// 半分が「完了した仕事」になること。その材料になるイベントが3つ — 開始 / 完了 / 失敗 — だけ。
//
// **不変条件（CLAUDE.md）: テレメトリに発話・文字起こし・応答を載せない。** PTTを通るものは
// 全て利用者の音声とそれへの回答なので、ここが運ぶのは時間（ms）と結果コードだけ。文字数すら
// 送らない — 長さが内容の指紋になり得るなら、それも情報だから。時間と enum コードがあれば
// 「速いか」「失敗していないか」には十分答えられる。
//
// opt-out の尊重は [`Analytics`] / [`AnalyticsHandle`] が単独責務として強制する（capture が
// opt_out 中に即破棄する）ので、ここでは重複チェックを書かない。setup で `Analytics` を manage
// する前に PTT 経路へ入り得るため、ハンドルは `try_state` で取り、無ければ no-op で抜ける。

/// push-to-talk のセッションが始まった（マイクが開いた）。
///
/// **発話も文字起こしも応答も送らない** — 送るのは時間と結果コードだけで、それで
/// 「速いか」「失敗していないか」は十分に分かる。文字数すら送らない（長さが内容の指紋に
/// なり得る）。
pub fn capture_ptt_started(app: &AppHandle) {
    if let Some(analytics) = app.try_state::<Analytics>() {
        analytics.capture("ptt_session_started", Props::new());
    }
}

/// 応答が最後まで届いた。`first_token_ms` が SLO-03（初トークン1s）の実測値。
///
/// `None` は「初トークンが1つも来なかった」— パネルが閉じられて受信を打ち切った場合。これを
/// `0` で潰すと「0msで初トークンが来た」と見分けが付かなくなるので、`Some` のときだけ
/// `first_token_ms` を載せる。値そのものが利用者の内容を運ぶことはない（時間だけ）。
pub fn capture_ptt_completed(app: &AppHandle, first_token_ms: Option<u64>, total_ms: u64) {
    if let Some(analytics) = app.try_state::<Analytics>() {
        let mut p = Props::new();
        p.insert("total_ms".into(), serde_json::Value::from(total_ms));
        if let Some(ms) = first_token_ms {
            p.insert("first_token_ms".into(), serde_json::Value::from(ms));
        }
        analytics.capture("ptt_session_completed", p);
    }
}

/// セッションが失敗して終わった。`code` は `shogun_core::ptt::statemachine::Fail::code()`
/// が返す安定した文字列（`mic_unavailable` / `no_asr_model` / `nothing_heard` /
/// `asr_failed` / `network` / `key_rejected`）。結果コード（enum 文字列）だけを載せる。
pub fn capture_ptt_failed(app: &AppHandle, code: &str) {
    if let Some(analytics) = app.try_state::<Analytics>() {
        let mut p = Props::new();
        p.insert("code".into(), serde_json::Value::from(code));
        analytics.capture("ptt_session_failed", p);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn context_props_carry_source_and_count() {
        let p = context_updated_props("gmail", 3);
        assert_eq!(p["source"], "gmail");
        assert_eq!(p["newly_inserted"], 3);
    }

    #[test]
    fn distinct_id_is_uuid_v4_shaped() {
        let id = new_distinct_id().unwrap();
        // 8-4-4-4-12 の 36 文字
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        // version nibble = 4
        assert_eq!(&id[14..15], "4");
        // variant nibble ∈ {8,9,a,b}
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn two_ids_differ() {
        assert_ne!(new_distinct_id().unwrap(), new_distinct_id().unwrap());
    }

    #[test]
    fn state_roundtrips_json_with_opt_out_default_false() {
        let json = r#"{"distinct_id":"x"}"#;
        let s: AnalyticsState = serde_json::from_str(json).unwrap();
        assert_eq!(s.distinct_id, "x");
        assert!(!s.opt_out); // #[serde(default)]
    }

    #[test]
    fn state_opt_out_flip_serializes() {
        let mut s = AnalyticsState { distinct_id: "x".into(), opt_out: false };
        s.opt_out = true;
        let json = serde_json::to_string(&s).unwrap();
        let back: AnalyticsState = serde_json::from_str(&json).unwrap();
        assert!(back.opt_out);
        assert_eq!(back.distinct_id, "x");
    }
}
