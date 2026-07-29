//! 匿名プロダクト分析（PostHog）。DAU/MAU 用の最小イベント送信。
//!
//! 大半は net 非依存の純ロジックで、Linux CI でテストできる。実 HTTPS 送信は
//! `net` feature の [`reqwest_transport::ReqwestTransport`] が担う。
//! CLAUDE.md invariant: 外部 egress は shogun-core に集約（FR-TR-03）。

use serde_json::{json, Value};

/// 1イベントのプロパティ集合。
pub type Props = serde_json::Map<String, Value>;

/// 送信待ちの1イベント。`props` は共通プロパティ込みの完成形。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub props: Props,
}

/// 送信先の固定設定（セッション内不変）。
#[derive(Debug, Clone)]
pub struct AnalyticsConfig {
    /// PostHog project write key（`phc_...`）。公開安全な書き込み専用キー。
    pub api_key: String,
    /// 匿名の distinct_id（永続マシンUUID）。
    pub distinct_id: String,
}

/// PostHog `/batch` に投げる JSON ボディを組み立てる純関数。
///
/// 形式: `{ "api_key": ..., "batch": [ { "event", "distinct_id", "properties" }, ... ] }`
/// v1 はタイムスタンプを付与せず PostHog サーバ受信時刻を採用する。
pub fn build_batch_payload(config: &AnalyticsConfig, events: &[Event]) -> String {
    let batch: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "event": e.name,
                "distinct_id": config.distinct_id,
                "properties": e.props,
            })
        })
        .collect();
    json!({ "api_key": config.api_key, "batch": batch }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AnalyticsConfig {
        AnalyticsConfig { api_key: "phc_test".into(), distinct_id: "abc-123".into() }
    }

    #[test]
    fn payload_has_api_key_and_one_batch_entry_per_event() {
        let mut props = Props::new();
        props.insert("cold_start".into(), Value::Bool(true));
        let events = vec![Event { name: "app_opened".into(), props }];

        let body = build_batch_payload(&cfg(), &events);
        let parsed: Value = serde_json::from_str(&body).unwrap();

        assert_eq!(parsed["api_key"], "phc_test");
        assert_eq!(parsed["batch"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["batch"][0]["event"], "app_opened");
        assert_eq!(parsed["batch"][0]["distinct_id"], "abc-123");
        assert_eq!(parsed["batch"][0]["properties"]["cold_start"], true);
    }

    #[test]
    fn empty_events_produce_empty_batch() {
        let body = build_batch_payload(&cfg(), &[]);
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["batch"].as_array().unwrap().len(), 0);
    }
}
