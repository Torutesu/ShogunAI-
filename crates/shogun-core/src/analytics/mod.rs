//! 匿名プロダクト分析（PostHog）。DAU/MAU 用の最小イベント送信。
//!
//! 大半は net 非依存の純ロジックで、Linux CI でテストできる。実 HTTPS 送信は
//! `net` feature の [`reqwest_transport::ReqwestTransport`] が担う。
//! CLAUDE.md invariant: 外部 egress は shogun-core に集約（FR-TR-03）。

use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "net")]
pub mod reqwest_transport;

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

/// バッチ送信の抽象。本番は reqwest、テストは Fake。
pub trait Transport: Send + 'static {
    /// 組み立て済み JSON ボディを送る。失敗は `Err(())`（本文は秘匿、内容は返さない）。
    /// Unit err は意図的：エラー内容を呼び出し元に漏らさない設計。
    #[allow(clippy::result_unit_err)]
    fn send_batch(&self, body: String) -> Result<(), ()>;
}

/// ワーカーの調律値。
const MAX_BATCH: usize = 20;
const FLUSH_INTERVAL: Duration = Duration::from_secs(3);
const MAX_RETRIES: u32 = 2;

/// 送信ワーカーのループ。`rx` が閉じたら残りをフラッシュして終了。
/// opt_out が true の間は受信イベントを破棄する。
pub fn run_worker<T: Transport>(
    rx: Receiver<Event>,
    config: AnalyticsConfig,
    transport: T,
    opt_out: Arc<AtomicBool>,
) {
    let mut buf: Vec<Event> = Vec::new();
    loop {
        match rx.recv_timeout(FLUSH_INTERVAL) {
            Ok(ev) => {
                if !opt_out.load(Ordering::Relaxed) {
                    buf.push(ev);
                    if buf.len() >= MAX_BATCH {
                        flush(&config, &transport, &mut buf);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !buf.is_empty() {
                    flush(&config, &transport, &mut buf);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !buf.is_empty() && !opt_out.load(Ordering::Relaxed) {
                    flush(&config, &transport, &mut buf);
                }
                return;
            }
        }
    }
}

fn flush<T: Transport>(config: &AnalyticsConfig, transport: &T, buf: &mut Vec<Event>) {
    let body = build_batch_payload(config, buf);
    for _ in 0..=MAX_RETRIES {
        if transport.send_batch(body.clone()).is_ok() {
            break;
        }
    }
    // 成否に関わらずバッファは破棄（無限肥大を防ぐ — リングバッファ相当）。
    buf.clear();
}

/// 呼び出し元が使うハンドル。Clone 可・全 clone が同じチャネルと opt_out を共有。
#[derive(Clone)]
pub struct AnalyticsHandle {
    tx: Sender<Event>,
    opt_out: Arc<AtomicBool>,
    /// 全イベントに合流させる共通プロパティ（os/app_version/plan 等）。
    base: Props,
}

impl AnalyticsHandle {
    /// transport を注入してワーカースレッドを起こし、ハンドルを返す。
    /// `opt_out` は Handle とワーカーで共有する（トグルが即時反映される）。
    pub fn spawn_shared<T: Transport>(
        config: AnalyticsConfig,
        base: Props,
        opt_out: Arc<AtomicBool>,
        transport: T,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Event>();
        let worker_opt_out = opt_out.clone();
        let _ = std::thread::Builder::new()
            .name("shogun-analytics".into())
            .spawn(move || run_worker(rx, config, transport, worker_opt_out));
        Self { tx, opt_out, base }
    }

    /// イベントを非ブロッキングで送る。opt_out 中は即破棄。共通プロパティを合流させる。
    pub fn capture(&self, event: &str, extra: Props) {
        if self.opt_out.load(Ordering::Relaxed) {
            return;
        }
        let mut props = self.base.clone();
        for (k, v) in extra {
            props.insert(k, v);
        }
        let _ = self.tx.send(Event { name: event.to_string(), props });
    }

    /// opt_out を即時切り替える（設定トグルから呼ぶ）。
    pub fn set_opt_out(&self, value: bool) {
        self.opt_out.store(value, Ordering::Relaxed);
    }

    /// v1 は口だけ用意（アカウント基盤到来時に匿名→アカウントIDのマージへ差し替え）。
    /// 現状は `$identify` イベントを1件送るだけ。
    pub fn identify(&self, account_id: &str) {
        let mut p = Props::new();
        p.insert("account_id".into(), Value::String(account_id.to_string()));
        self.capture("$identify", p);
    }
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

    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// 送られたボディを記録する Fake。
    #[derive(Clone, Default)]
    struct FakeTransport {
        sent: Arc<Mutex<Vec<String>>>,
    }
    impl Transport for FakeTransport {
        fn send_batch(&self, body: String) -> Result<(), ()> {
            self.sent.lock().unwrap().push(body);
            Ok(())
        }
    }

    #[test]
    fn capture_merges_base_props_and_flushes_on_shutdown() {
        let mut base = Props::new();
        base.insert("app_version".into(), "0.0.0".into());
        let opt_out = Arc::new(AtomicBool::new(false));
        let fake = FakeTransport::default();
        let sent = fake.sent.clone();

        let handle = AnalyticsHandle::spawn_shared(cfg(), base, opt_out.clone(), fake);
        let mut p = Props::new();
        p.insert("cold_start".into(), serde_json::Value::Bool(true));
        handle.capture("app_opened", p);
        drop(handle); // チャネルを閉じてワーカーを終了 → 最終フラッシュ

        for _ in 0..50 {
            if !sent.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let bodies = sent.lock().unwrap();
        assert_eq!(bodies.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(parsed["batch"][0]["event"], "app_opened");
        assert_eq!(parsed["batch"][0]["properties"]["app_version"], "0.0.0");
        assert_eq!(parsed["batch"][0]["properties"]["cold_start"], true);
    }

    #[test]
    fn opt_out_drops_events() {
        let opt_out = Arc::new(AtomicBool::new(true));
        let fake = FakeTransport::default();
        let sent = fake.sent.clone();
        let handle = AnalyticsHandle::spawn_shared(cfg(), Props::new(), opt_out.clone(), fake);
        handle.capture("app_opened", Props::new());
        drop(handle);
        std::thread::sleep(Duration::from_millis(120));
        assert!(sent.lock().unwrap().is_empty());
    }
}
