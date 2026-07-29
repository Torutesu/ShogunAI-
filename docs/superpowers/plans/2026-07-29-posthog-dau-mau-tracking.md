# PostHog DAU/MAU トラッキング Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ShogunAI デスクトップアプリから `app_opened` / `shogun_query_executed` / `context_updated` の3イベントを匿名で PostHog に送り、DAU/MAU/スティッキネスを PostHog 上で集計できるようにする。

**Architecture:** 送信ロジックは `shogun-core::analytics`（純ロジック＋Transport抽象、大半は Linux CI でテスト可）。実際の HTTPS 送信は `net` feature の `ReqwestTransport`。Tauri シェル（`apps/desktop/src-tauri/src/analytics.rs`）が `analytics.json`（distinct_id＋opt_out）・環境変数・共通プロパティを組み立ててハンドルを生成し、3つの発火点に配線する。`SHOGUN_POSTHOG_KEY` 未設定または opt_out 時は完全 no-op。

**Tech Stack:** Rust / Tauri v2 / reqwest(blocking, rustls, `net` feature) / serde_json / std::thread + std::sync::mpsc（送信ワーカー）/ getrandom（distinct_id 生成）/ React（opt-out トグル）。

**参照 spec:** `docs/superpowers/specs/2026-07-29-posthog-dau-mau-tracking-design.md`

---

## File Structure

**新規（shogun-core）**
- `crates/shogun-core/src/analytics/mod.rs` — 型（`Event`/`Props`/`AnalyticsConfig`）、純関数 `build_batch_payload`、`Transport` trait、ワーカーループ `run_worker`、`AnalyticsHandle`、`spawn`。net 非依存部。
- `crates/shogun-core/src/analytics/reqwest_transport.rs` — `#[cfg(feature = "net")]` の `ReqwestTransport`（PostHog `/batch` への blocking POST）。

**修正（shogun-core）**
- `crates/shogun-core/src/lib.rs` — `pub mod analytics;` を追加。

**新規（desktop シェル）**
- `apps/desktop/src-tauri/src/analytics.rs` — `Analytics`（`Option<AnalyticsHandle>` ラッパ、no-op安全）、`analytics.json` の load/save、distinct_id 生成、共通プロパティ、`init()`、opt-out Tauri コマンド。

**修正（desktop シェル）**
- `apps/desktop/src-tauri/src/lib.rs` — `mod analytics;`、setup で `Analytics` を生成→`manage`→`app_opened` 発火、バス購読タスク起動、opt-out コマンドを `invoke_handler` に登録。
- `apps/desktop/src-tauri/src/notch_exec.rs` — `run_notch_action` に `shogun_query_executed` 発火を追加。

**新規（frontend）**
- `apps/desktop/src/AnalyticsToggle.tsx` — 「匿名の利用状況を送信」トグル（設定/オンボーディングに配置）。

**PostHog 側（本計画のコード外・環境確定後）**
- ダッシュボード `ShogunAI – Core KPI (DAU/MAU)` を PostHog MCP で構築（末尾「PostHog ダッシュボード構築」参照）。

---

## Task 1: analytics コア型と `build_batch_payload`（純関数）

**Files:**
- Create: `crates/shogun-core/src/analytics/mod.rs`
- Modify: `crates/shogun-core/src/lib.rs`（`pub mod analytics;` 追加）

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/analytics/mod.rs` を新規作成し、まず型・純関数の骨組みとテストを置く：

```rust
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
```

`crates/shogun-core/src/lib.rs` のモジュール宣言群（`pub mod bus;` 等の並び）に追加：

```rust
pub mod analytics;
```

- [ ] **Step 2: テストが失敗（または未コンパイル）することを確認**

Run: `cargo test -p shogun-core analytics::tests 2>&1 | tail -20`
Expected: この時点ではコンパイル成功しテストは PASS するはず（純関数のみ）。もしモジュール未配線なら「module not found」で失敗 → Step 1 の `pub mod analytics;` を確認。

- [ ] **Step 3: テストが通ることを確認**

Run: `cargo test -p shogun-core analytics:: 2>&1 | tail -20`
Expected: `payload_has_api_key...` と `empty_events...` が PASS。

- [ ] **Step 4: コミット**

```bash
git add crates/shogun-core/src/analytics/mod.rs crates/shogun-core/src/lib.rs
git commit -m "feat(analytics): core types and build_batch_payload (#61)"
```

---

## Task 2: Transport 抽象・ワーカーループ・AnalyticsHandle（net 非依存, FakeTransport でテスト）

**Files:**
- Modify: `crates/shogun-core/src/analytics/mod.rs`

送信ワーカーは `std::thread` + `std::sync::mpsc` の blocking 実装。opt_out は `Arc<AtomicBool>` を Handle とワーカーで共有し、即時反映。Transport を trait 化して Fake で全ロジックをテストする。

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/analytics/mod.rs` の `use` に追記し、型定義の後ろ（`#[cfg(test)]` の前）に以下を追加：

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// バッチ送信の抽象。本番は reqwest、テストは Fake。
pub trait Transport: Send + 'static {
    /// 組み立て済み JSON ボディを送る。失敗は `Err(())`（本文は秘匿、内容は返さない）。
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

// impl（spawn_shared / capture / set_opt_out / identify）は Step 3 で追加する。
// この Step ではまだ未実装なので、下のテストが参照する `spawn_shared` が無く RED になる。
```

> **注意**: `Sender`/`Receiver`/`Mutex` など未使用 use による warning-as-error を避けるため、`impl` を追加する Step 3 まで一時的に `#[allow(unused_imports)]` を `use` 群の直前に付けてよい（Step 3 で外す）。

同ファイルの `#[cfg(test)] mod tests` に追加：

```rust
    use std::sync::atomic::{AtomicBool, Ordering};
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

        let handle =
            AnalyticsHandle::spawn_shared(cfg(), base, opt_out.clone(), fake);
        let mut p = Props::new();
        p.insert("cold_start".into(), serde_json::Value::Bool(true));
        handle.capture("app_opened", p);
        drop(handle); // チャネルを閉じてワーカーを終了 → 最終フラッシュ

        // ワーカースレッドの完了を待つ簡易ポーリング。
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
        let handle =
            AnalyticsHandle::spawn_shared(cfg(), Props::new(), opt_out.clone(), fake);
        handle.capture("app_opened", Props::new());
        drop(handle);
        std::thread::sleep(Duration::from_millis(120));
        assert!(sent.lock().unwrap().is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p shogun-core analytics:: 2>&1 | tail -30`
Expected: コンパイルエラー（`spawn_shared` 未定義、`unreachable!` の仮メソッド）で FAIL。

- [ ] **Step 3: 最小実装（仮メソッドを `spawn_shared` と `capture` に置換）**

`impl AnalyticsHandle` ブロック全体を以下に置換：

```rust
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
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p shogun-core analytics:: 2>&1 | tail -30`
Expected: 4テスト（Task1の2 + 本タスクの2）すべて PASS。

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/analytics/mod.rs
git commit -m "feat(analytics): worker, Transport trait, AnalyticsHandle (#61)"
```

---

## Task 3: ReqwestTransport（`net` feature）

**Files:**
- Create: `crates/shogun-core/src/analytics/reqwest_transport.rs`
- Modify: `crates/shogun-core/src/analytics/mod.rs`（`#[cfg(feature = "net")] pub mod reqwest_transport;` 追加）

- [ ] **Step 1: 実装を書く**

`crates/shogun-core/src/analytics/reqwest_transport.rs`:

```rust
//! PostHog `/batch` への blocking POST（feature `net`）。
//!
//! 既存の `mcp_http.rs` と同じ方針: reqwest blocking + rustls、証明書検証は無効化しない
//! （NFR-SEC-04）。エラー本文は秘匿（リクエスト/レスポンス本文を surface しない）。

use super::Transport;

/// PostHog キャプチャ用の blocking HTTP transport。
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
    /// 送信先 URL（`<host>/batch/`）。
    url: String,
}

impl ReqwestTransport {
    /// host（例 `https://us.i.posthog.com`）から transport を組む。
    /// TLS 初期化に失敗したら `None`（analytics は no-op に落とす）。
    pub fn new(host: &str) -> Option<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("shogun/1.0")
            .build()
            .ok()?;
        let url = format!("{}/batch/", host.trim_end_matches('/'));
        Some(Self { client, url })
    }
}

impl Transport for ReqwestTransport {
    fn send_batch(&self, body: String) -> Result<(), ()> {
        let resp = self
            .client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .map_err(|_| ())?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(())
        }
    }
}
```

`crates/shogun-core/src/analytics/mod.rs` の先頭付近（`use` の後）に追加：

```rust
#[cfg(feature = "net")]
pub mod reqwest_transport;
```

- [ ] **Step 2: net feature でコンパイルを確認**

Run: `cargo build -p shogun-core --features net 2>&1 | tail -20`
Expected: エラーなくビルド成功。

- [ ] **Step 3: net 無しでもコンパイル通ることを確認（純ロジックCIの回帰防止）**

Run: `cargo build -p shogun-core 2>&1 | tail -10`
Expected: エラーなくビルド成功（reqwest_transport は cfg で除外）。

- [ ] **Step 4: コミット**

```bash
git add crates/shogun-core/src/analytics/reqwest_transport.rs crates/shogun-core/src/analytics/mod.rs
git commit -m "feat(analytics): ReqwestTransport for PostHog /batch (feature net) (#61)"
```

---

## Task 4: シェルの analytics.json（distinct_id 生成・opt_out 永続化）

**Files:**
- Create: `apps/desktop/src-tauri/src/analytics.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`（`mod analytics;` 追加）

- [ ] **Step 1: 失敗するテストを書く**

`apps/desktop/src-tauri/src/analytics.rs` を新規作成：

```rust
//! Tauri シェル側の分析アダプタ。`analytics.json`（distinct_id + opt_out）の永続化、
//! distinct_id の生成、共通プロパティ組み立て、ハンドル生成、opt-out コマンドを担う。
//!
//! 送信ロジック本体は `shogun_core::analytics`。ここは OS/設定/配線だけ。

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
```

`apps/desktop/src-tauri/src/lib.rs` のモジュール宣言群（`mod notch_exec;` などの並び）に追加：

```rust
mod analytics;
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p shogun-desktop-spike analytics:: 2>&1 | tail -20`
Expected: 初回はコンパイル成功しテスト PASS になる想定（純ロジック）。もし `getrandom` 解決エラーなら desktop の `Cargo.toml` に既存の `getrandom = "0.2"` があることを確認（既存）。

> macOS 以外では `shogun-desktop-spike` はビルド対象外（Cargo.toml 注記）。本タスクのテストは macOS 上で実行する。

- [ ] **Step 3: テストが通ることを確認**

Run: `cargo test -p shogun-desktop-spike analytics:: 2>&1 | tail -20`
Expected: 3テスト PASS。

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src-tauri/src/analytics.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(analytics): shell state + distinct_id minting (#61)"
```

---

## Task 5: シェルの `init()` と load/save・共通プロパティ・`Analytics` ラッパ

**Files:**
- Modify: `apps/desktop/src-tauri/src/analytics.rs`

既存の `onboarding.rs` の JSON load/save パターン（`app.path().app_data_dir()` → `join("*.json")` → `fs::read_to_string`/`fs::write`）に倣う。

- [ ] **Step 1: 実装を追加**

`apps/desktop/src-tauri/src/analytics.rs` の `new_distinct_id` の後、`#[cfg(test)]` の前に追加：

```rust
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use shogun_core::analytics::{AnalyticsConfig, AnalyticsHandle, Props};
use shogun_core::analytics::reqwest_transport::ReqwestTransport;

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
    p.insert("os".into(), std::env::consts::OS.into());
    p.insert("app_version".into(), app.package_info().version.to_string().into());
    // v1 は課金基盤前のため "trial" 固定（fullui.rs と同じ実態）。
    p.insert("plan".into(), "trial".into());
    p
}

/// 分析を初期化する。`SHOGUN_POSTHOG_KEY` 未設定なら無効（no-op）ラッパを返す。
pub fn init(app: &AppHandle) -> Analytics {
    let key = std::env::var("SHOGUN_POSTHOG_KEY").unwrap_or_default();
    if key.is_empty() {
        eprintln!("[analytics] SHOGUN_POSTHOG_KEY unset — analytics disabled");
        return Analytics(None);
    }
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
```

- [ ] **Step 2: コンパイル確認（テストは既存の3件が維持されること）**

Run: `cargo test -p shogun-desktop-spike analytics:: 2>&1 | tail -20`
Expected: 既存3テスト PASS、コンパイル成功。

- [ ] **Step 3: コミット**

```bash
git add apps/desktop/src-tauri/src/analytics.rs
git commit -m "feat(analytics): init(), state load/save, base props, Analytics wrapper (#61)"
```

---

## Task 6: setup で初期化＋`app_opened` 発火、`Analytics` を manage

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: setup 内に配線を追加**

`apps/desktop/src-tauri/src/lib.rs` の Tauri `setup`（`setup_macos` 相当）で、`Db`/`NotchEngine` などを `app.manage(...)` している箇所の近く、setup 完了直前に以下を追加。`app` は `&AppHandle`（`app.handle()` で取得可能な文脈に合わせる）。

```rust
// --- 匿名プロダクト分析（PostHog, #61）---
let analytics = crate::analytics::init(&app.handle());
{
    let mut p = shogun_core::analytics::Props::new();
    p.insert("cold_start".into(), serde_json::Value::Bool(true));
    analytics.capture("app_opened", p);
}
app.manage(analytics);
```

> 配置メモ: `app.manage(...)` は他の state 登録と同じブロックで行う。`app_opened` は setup で1回だけ通るため launch あたり1回になる（spec のガード要件を満たす）。

- [ ] **Step 2: ビルド確認**

Run: `cargo build -p shogun-desktop-spike 2>&1 | tail -20`
Expected: ビルド成功。`app.handle()` の型不一致が出た場合は、周辺の既存 `manage` 呼び出しが使っている app 参照（`app` か `app.handle()`）に合わせる。

- [ ] **Step 3: 手動スモーク（任意・鍵未設定でクラッシュしないこと）**

Run: `cargo build -p shogun-desktop-spike 2>&1 | tail -5`（`SHOGUN_POSTHOG_KEY` 未設定）
Expected: 起動系がビルドでき、init が `analytics disabled` を eprintln して no-op になる設計であることをコードで確認。

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(analytics): init + app_opened in setup, manage Analytics state (#61)"
```

---

## Task 7: `shogun_query_executed`（notch_exec）

**Files:**
- Modify: `apps/desktop/src-tauri/src/notch_exec.rs`

- [ ] **Step 1: `run_notch_action` に発火を追加**

`apps/desktop/src-tauri/src/notch_exec.rs` の `run_notch_action` シグネチャに `analytics` state を追加し、`submit` 後・return 前に発火する。差し替え後の関数：

```rust
    #[tauri::command]
    pub fn run_notch_action(
        index: usize,
        db: tauri::State<'_, Db>,
        engine: tauri::State<'_, NotchEngine>,
        analytics: tauri::State<'_, crate::analytics::Analytics>,
    ) -> String {
        let cache = db.context_actions(current_screen(), None);
        let Some(cand) = cache.actions.get(index) else {
            return "no-action".to_string();
        };
        let Ok(mut eng) = engine.lock() else {
            return "unavailable".to_string();
        };
        let submitted = eng.submit(cand.action.clone(), now_ms());

        // shogun_query_executed（#61）: submit した時のみ発火。
        let outcome = match &submitted.disposition {
            Disposition::AutoRan => "ok",
            Disposition::AwaitingConfirm => "awaiting_confirm",
            Disposition::Rejected(_) => "rejected",
        };
        let mut p = shogun_core::analytics::Props::new();
        p.insert("query_type".into(), "notch_action".into());
        p.insert("permission_level".into(), format!("{:?}", cand.level).into());
        p.insert("outcome".into(), outcome.into());
        analytics.capture("shogun_query_executed", p);

        match submitted.disposition {
            Disposition::AutoRan => "executed".to_string(),
            Disposition::AwaitingConfirm => format!("confirm:{}", submitted.id.0),
            Disposition::Rejected(_) => "rejected".to_string(),
        }
    }
```

> `cand.level` の Debug 表記が `L1`/`L2`/`L3` 以外（例 `Level::L1`）の場合は、`permission_level` の値がその文字列になる。ダッシュボードのセグメントはこの実値に合わせる。厳密な短縮が必要なら別途 `match cand.level` で明示マップに変更（v1 は Debug で可）。

- [ ] **Step 2: ビルド確認**

Run: `cargo build -p shogun-desktop-spike 2>&1 | tail -20`
Expected: ビルド成功。`run_notch_action` を呼ぶ `invoke_handler` 側は引数追加の影響を受けない（state は自動注入）。

- [ ] **Step 3: 既存テストの回帰確認**

Run: `cargo test -p shogun-desktop-spike 2>&1 | tail -20`
Expected: 既存テスト PASS。

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src-tauri/src/notch_exec.rs
git commit -m "feat(analytics): fire shogun_query_executed on notch action (#61)"
```

---

## Task 8: `context_updated`（バス購読タスク）

**Files:**
- Modify: `apps/desktop/src-tauri/src/analytics.rs`（純マッパ + テスト）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（購読タスク起動）

まず純マッパをテスト付きで足し、次に配線する。

- [ ] **Step 1: 失敗するテスト（純マッパ）を書く**

`apps/desktop/src-tauri/src/analytics.rs` に追加：

```rust
/// `IntegrationSynced { source, count }` → context_updated のプロパティに変換する純関数。
pub fn context_updated_props(source: &str, count: u64) -> shogun_core::analytics::Props {
    let mut p = shogun_core::analytics::Props::new();
    p.insert("source".into(), source.into());
    p.insert("newly_inserted".into(), serde_json::json!(count));
    p
}
```

`#[cfg(test)] mod tests` に追加：

```rust
    #[test]
    fn context_props_carry_source_and_count() {
        let p = context_updated_props("gmail", 3);
        assert_eq!(p["source"], "gmail");
        assert_eq!(p["newly_inserted"], 3);
    }
```

- [ ] **Step 2: テスト失敗→通過を確認**

Run: `cargo test -p shogun-desktop-spike analytics:: 2>&1 | tail -20`
Expected: 追加テスト含め PASS。

- [ ] **Step 3: バス購読タスクを配線**

まず `apps/desktop/src-tauri/src/lib.rs` で `Bus` インスタンスがシェル内に存在するか確認する：

Run: `grep -n "Bus::new\|bus\.subscribe\|shogun_core::bus\|: Bus\b" apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/connectors.rs 2>/dev/null`

- **(A) シェルに `Bus` ハンドルがある場合**: setup で `Analytics` を manage した後、購読タスクを spawn（既存の tokio ランタイム上）：

```rust
// context_updated（#61）: バスの IntegrationSynced を購読して送る。
{
    let analytics = app.state::<crate::analytics::Analytics>().inner().clone();
    let mut sub = bus.subscribe(); // `bus` は setup で構築済みの shogun_core::bus::Bus
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = sub.recv().await {
            if let shogun_core::bus::BusEvent::IntegrationSynced { source, count } = &*ev {
                analytics.capture("context_updated", crate::analytics::context_updated_props(source, *count));
            }
        }
    });
}
```

- **(B) シェルに `Bus` ハンドルが無い場合**: `apps/desktop/src-tauri/src/connectors.rs` の read-sync 完了点（sync 件数が確定する箇所）を特定し、そこで直接発火する。`Analytics` を connectors ランタイムに渡せない場合は、`app.state::<crate::analytics::Analytics>()` を完了ハンドラのスコープで取得して：

```rust
if let Some(analytics) = app.try_state::<crate::analytics::Analytics>() {
    analytics.capture("context_updated", crate::analytics::context_updated_props(source, count));
}
```

> どちらの経路でも `context_updated_props`（テスト済み）を使う。まず Step 3 の grep で (A)/(B) を判定してから実装する。

- [ ] **Step 4: ビルド確認**

Run: `cargo build -p shogun-desktop-spike 2>&1 | tail -20`
Expected: ビルド成功。

- [ ] **Step 5: コミット**

```bash
git add apps/desktop/src-tauri/src/analytics.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(analytics): fire context_updated from integration sync (#61)"
```

---

## Task 9: opt-out コマンド＋永続化、フロントのトグル

**Files:**
- Modify: `apps/desktop/src-tauri/src/analytics.rs`（コマンド2つ + テスト）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（`invoke_handler` に登録）
- Create: `apps/desktop/src/AnalyticsToggle.tsx`

- [ ] **Step 1: コマンドを実装**

`apps/desktop/src-tauri/src/analytics.rs` に追加：

```rust
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
```

`#[cfg(test)] mod tests` に、状態更新の純ロジック確認を追加（AppHandle を要さない範囲）：

```rust
    #[test]
    fn state_opt_out_flip_serializes() {
        let mut s = AnalyticsState { distinct_id: "x".into(), opt_out: false };
        s.opt_out = true;
        let json = serde_json::to_string(&s).unwrap();
        let back: AnalyticsState = serde_json::from_str(&json).unwrap();
        assert!(back.opt_out);
        assert_eq!(back.distinct_id, "x");
    }
```

- [ ] **Step 2: `invoke_handler` に登録**

`apps/desktop/src-tauri/src/lib.rs` の `tauri::generate_handler![ ... ]` に追加：

```rust
            crate::analytics::analytics_get_opt_out,
            crate::analytics::analytics_set_opt_out,
```

- [ ] **Step 3: テスト＆ビルド確認**

Run: `cargo test -p shogun-desktop-spike analytics:: 2>&1 | tail -20 && cargo build -p shogun-desktop-spike 2>&1 | tail -10`
Expected: analytics テスト全 PASS、ビルド成功。

- [ ] **Step 4: フロントのトグルを作る**

`apps/desktop/src/AnalyticsToggle.tsx`:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * 「匿名の利用状況を送信」トグル（オプトアウト方式・既定ON）。
 * 設定 or オンボーディングに配置する。
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
```

- [ ] **Step 5: トグルを設定画面/オンボーディングに配置**

既存の設定 UI を特定して `AnalyticsToggle` をマウントする：

Run: `grep -rn "invoke(\|Settings\|settings\|onboarding" apps/desktop/src --include="*.tsx" | grep -iv "test" | head`

見つけた設定/オンボーディングコンポーネントに `import { AnalyticsToggle } from "./AnalyticsToggle";` を足し、適切な場所に `<AnalyticsToggle />` を置く。専用の設定画面が無ければ、初回オンボーディングの完了画面に配置する。

- [ ] **Step 6: フロントの型チェック**

Run: `cd apps/desktop && pnpm typecheck 2>&1 | tail -20`
Expected: 型エラーなし。

- [ ] **Step 7: コミット**

```bash
git add apps/desktop/src-tauri/src/analytics.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src/AnalyticsToggle.tsx
git commit -m "feat(analytics): opt-out commands + frontend toggle (#61)"
```

---

## Task 10: 結合スモーク（ローカル受信スタブに3イベントが飛ぶ）

**Files:**
- 一時的なローカル受信のみ（コード変更なし、確認用）

- [ ] **Step 1: ローカル受信スタブを立てる**

任意の HTTP エコー（例: `python3 -m http.server` はPOST不可のため、簡易に nc で受信確認）：

Run:
```bash
# ターミナルAで受信を待つ（1リクエストを表示）
while true; do printf 'HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok' | nc -l 8010; echo "--- got request ---"; done
```

- [ ] **Step 2: スタブ host を指してアプリを起動**

Run:
```bash
SHOGUN_POSTHOG_KEY=phc_dummy SHOGUN_POSTHOG_HOST=http://localhost:8010 cargo run -p shogun-desktop-spike 2>&1 | tail -30
```
Expected: 起動時にターミナルAへ `POST /batch/` が届き、ボディに `"event":"app_opened"` と `"distinct_id"`、`"properties":{"os":"macos","app_version":...,"plan":"trial","cold_start":true}` が含まれる。

- [ ] **Step 3: 手動でノッチアクション実行 → `shogun_query_executed` を確認**

アプリでノッチのコンテキストアクションを1つ実行し、ターミナルAに `"event":"shogun_query_executed"`（`query_type":"notch_action"`, `permission_level`, `outcome`）が届くことを確認。

- [ ] **Step 4: opt-out トグルOFF（送信停止）を確認**

設定のトグルで送信をOFF（opt_out=true）にし、以降アクションを実行してもターミナルAに新規リクエストが来ないこと、`analytics.json` の `opt_out` が `true` になっていることを確認：

Run: `cat "$(find ~/Library/Application\ Support -name analytics.json 2>/dev/null | head -1)"`
Expected: `{"distinct_id":"...","opt_out":true}`

- [ ] **Step 5: 確認結果を記録（コミット不要）**

3イベントが正しい形で飛び、opt-out で停止することを確認できたら本タスク完了。

---

## PostHog ダッシュボード構築（環境確定後・コード外）

`SHOGUN_POSTHOG_KEY`/`SHOGUN_POSTHOG_HOST`（＝プロジェクト）が確定し、実イベントが1日以上蓄積したら、PostHog MCP で以下を構築する（別セッションで実施可）：

1. プロジェクト TZ を **Asia/Tokyo (JST)** に設定。
2. Insight を作成：
   - Yesterday DAU: 3イベント OR の日次 unique users、最新値カード。
   - This Month MAU: 30日ローリング unique users。
   - DAU/MAU %: Formula（DAU ÷ MAU）。
   - 日次DAU推移: 直近30–90日 折れ線。
   - 月次MAU推移: データ蓄積次第。
3. ダッシュボード `ShogunAI – Core KPI (DAU/MAU)` にカードを配置。セグメント: `plan`, `app_version`。
4. アクセス権: 全メンバー閲覧、編集は PM/ファウンダー/データ担当。URL を定例アジェンダに固定。
5. 簡易アラート Insight: `app_opened` 件数が閾値を下回ったら通知（Slack連携は Phase 2）。

---

## Self-Review 結果

- **Spec カバレッジ**: アクティブ定義3イベント（Task6/7/8）、distinct_id 匿名UUID（Task4）、opt-out既定ON＋開示（Task9）、Rustバックエンド送信・単一egress（Task1–3、shogun-core集約）、env注入・no-op（Task5）、TZ/ダッシュボード（末尾セクション）、信頼性（Task2のリトライ＋バッファ破棄）— すべて対応タスクあり。
- **プレースホルダ**: なし（各ステップに実コード・実コマンド記載）。
- **型整合**: `Props`/`Event`/`AnalyticsConfig`/`AnalyticsHandle::spawn_shared`/`capture`/`set_opt_out`/`Transport::send_batch`/`context_updated_props`/`Analytics` の名称は全タスクで一致。Task2 の仮メソッド（`spawn`/`spawn_with`）は Step3 で削除し `spawn_shared` に一本化する旨を明記済み。
- **不確実点の明示**: Task8 のバス経路 (A)/(B)、Task7 の `cand.level` Debug 表記は grep/確認ステップ付きで分岐を明示。
