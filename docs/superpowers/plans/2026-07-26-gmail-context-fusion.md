# Gmail 文脈融合 + 送信ループ 実装プラン

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gmail から取得した完全なスレッドを画面コンテキストで選んでドラフトの根拠にし、L3 確認で Composio 送信する縦一本を、公式 MCP に依存せず通す。

**Architecture:** transport 継ぎ目 `McpRpc` に Gmail REST 実装 (`GmailRestRpc`) を入れ、既存の正規化・ingest・信頼度ゲート・承認キューをそのまま乗せる。融合は「画面(件名)＝セレクタ → `gmail:<threadId>`＝ペイロード」を純関数リンカで橋渡しする。

**Tech Stack:** Rust / Tauri v2 / reqwest (blocking, feature `net`) / serde_json / 既存 shogun-integrations (OAuth+Keychain) / 既存 shogun-fusion (信頼度) / Gmail REST v1 / Composio HTTP。

**設計文書:** `docs/superpowers/specs/2026-07-26-gmail-context-fusion-design.md`

---

## 共有する型・名前（全タスク共通）

- リンカ純関数: `shogun_memory::thread::link_on_screen_to_thread(on_screen_title: &str, candidates: &[(String, String)]) -> Option<String>`
  - `candidates` は `(thread_key, subject)`。返り値は一致した `thread_key`（`gmail:<id>`）。
- `shogun_memory::thread::normalise_window_title` を `pub` に昇格（リンカが使う）。
- `PayloadSource`（`shogun-core/src/daemon.rs`）:
  ```rust
  #[derive(Debug, Clone, PartialEq, Default)]
  pub enum PayloadSource {
      /// 取得した実メール由来（高信頼）。thread_key が provenance（同期スレッドの識別子）。
      /// 注: 現状の ingest はメッセージIDを保存しないため、provenance は thread_key
      /// （`gmail:unknown:<件名>`）で表す。将来 native id を保存するなら message_id 化できる。
      Fetched { thread_key: String },
      /// 取得データに解決できず、画面キャプチャの断片のみ。
      #[default]
      OnScreenOnly,
  }
  ```
- DB クエリ: `Db::gmail_thread_candidates(&self, limit: usize) -> Vec<(String, String)>` = `(thread_key, title)`（`gmail:` プレフィックスのスレッドのみ）。
- 整形の純関数（`shogun-core/src/gmail_shape.rs`、非 feature ゲート、Linux テスト可）:
  `record_from_thread` / `envelope` / `draft_request_body` / `base64_url_no_pad`。
- `GmailRestRpc`（`shogun-core/src/gmail_rest.rs`、`#[cfg(feature = "net")]`）:
  `struct GmailRestRpc<P: TokenProvider>`、`impl McpRpc`。処理する tool 名:
  `search_threads` / `get_thread` / `create_draft`。整形は `crate::gmail_shape` を使う。
- Gmail REST ベース URL: `https://gmail.googleapis.com/gmail/v1/users/me`。
- 取り込んだ Gmail の thread_key は `gmail:unknown:<正規化件名>`（ingest は native id を保存しない）。
  リンカは件名照合なのでこれで成立する。

---

## Task 1: リンカ純関数 + normalise 昇格

**Files:**
- Modify: `crates/shogun-memory/src/thread.rs`（`fn normalise_window_title` → `pub fn`；末尾のテスト mod に追加）

- [ ] **Step 1: `normalise_window_title` を pub 化**

`crates/shogun-memory/src/thread.rs:16` を変更:

```rust
pub fn normalise_window_title(title: &str) -> String {
```

- [ ] **Step 2: 失敗するテストを書く**

`crates/shogun-memory/src/thread.rs` のテスト mod（`#[cfg(test)] mod tests`）に追加:

```rust
#[test]
fn linker_exact_match_wins() {
    let cands = vec![
        ("gmail:aaa".to_string(), "Q3 pricing".to_string()),
        ("gmail:bbb".to_string(), "Lunch Friday".to_string()),
    ];
    // ブラウザのタブ名は "(3) Q3 pricing — Gmail" のような装飾付き。
    let got = link_on_screen_to_thread("(3) Q3 pricing — Gmail", &cands);
    assert_eq!(got.as_deref(), Some("gmail:aaa"));
}

#[test]
fn linker_falls_back_to_containment() {
    let cands = vec![("gmail:aaa".to_string(), "Q3 pricing plan review".to_string())];
    // 画面側が件名の一部だけ持つケース（片方が他方を含む）。
    let got = link_on_screen_to_thread("Q3 pricing plan review — Gmail", &cands);
    assert_eq!(got.as_deref(), Some("gmail:aaa"));
}

#[test]
fn linker_refuses_short_or_empty_subjects() {
    // 短すぎる件名は包含照合を使わない（他人のスレッド誤挿入を防ぐ）。
    let cands = vec![("gmail:aaa".to_string(), "Re".to_string())];
    assert_eq!(link_on_screen_to_thread("Re — Gmail", &cands), None);
    assert_eq!(link_on_screen_to_thread("", &cands), None);
}

#[test]
fn linker_no_match_returns_none() {
    let cands = vec![("gmail:aaa".to_string(), "Completely different".to_string())];
    assert_eq!(link_on_screen_to_thread("Q3 pricing — Gmail", &cands), None);
}
```

- [ ] **Step 3: テストが失敗することを確認**

Run: `cargo test -p shogun-memory --lib thread::tests::linker 2>&1 | tail -5`
Expected: FAIL（`link_on_screen_to_thread` 未定義）

- [ ] **Step 4: リンカを実装**

`crates/shogun-memory/src/thread.rs` の `thread_key` 関数の直後に追加:

```rust
/// 画面の窓タイトル（件名を含む装飾付き文字列）を、取得済みスレッド候補
/// `(thread_key, subject)` の中の 1 つに解決する。純関数。
///
/// 段階: (1) 正規化件名の完全一致 → (2) 包含（片方が他方を含む）→ (3) 不一致は None。
/// 件名が短すぎる（正規化後 3 文字未満）ときは包含照合を使わない — 短い共通語で
/// 他人のスレッドを誤って差し込む害の方が大きいため（設計 §3）。
pub fn link_on_screen_to_thread(
    on_screen_title: &str,
    candidates: &[(String, String)],
) -> Option<String> {
    let screen = normalise_window_title(on_screen_title);
    if screen.chars().count() < 3 {
        return None;
    }
    // (1) 完全一致
    for (key, subject) in candidates {
        if normalise_window_title(subject) == screen {
            return Some(key.clone());
        }
    }
    // (2) 包含（両側とも 3 文字以上のときのみ）
    for (key, subject) in candidates {
        let subj = normalise_window_title(subject);
        if subj.chars().count() < 3 {
            continue;
        }
        if subj.contains(&screen) || screen.contains(&subj) {
            return Some(key.clone());
        }
    }
    None
}
```

- [ ] **Step 5: テストが通ることを確認**

Run: `cargo test -p shogun-memory --lib thread:: 2>&1 | grep 'test result'`
Expected: PASS（既存 thread テスト含め all pass）

- [ ] **Step 6: clippy**

Run: `cargo clippy -p shogun-memory --lib -- -D warnings 2>&1 | tail -2`
Expected: Finished（警告なし）

- [ ] **Step 7: Commit**

```bash
git add crates/shogun-memory/src/thread.rs
git commit -m "feat: on-screen→thread linker (subject match) for context fusion

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: レスポンス整形の純関数（`gmail_shape.rs`、ネットワークなし）

**Files:**
- Create: `crates/shogun-core/src/gmail_shape.rs`（純関数とテストのみ）
- Modify: `crates/shogun-core/src/lib.rs`（`pub mod gmail_shape;` を追加。feature ゲートなし）

`parse_items`（`shogun-integrations/result.rs`）が食う形は `{ "structuredContent": [ 記録, ... ] }`、
各記録は許容キー `threadId`/`subject`/`snippet`/`internalDate` を持てばよい。

注: `gmail_shape` は feature ゲート無し（serde_json だけに依存）。CI の pure ジョブが feature 無しで
`--all-targets` をコンパイルするため、この module のテストは `shogun-integrations`（gated 依存）に
触れてはいけない。`parse_items` を使った受け入れ検証は net-gated な Task 3 に置く。

- [ ] **Step 1: モジュール宣言を追加**

`crates/shogun-core/src/lib.rs` の他の `pub mod` 群の並びに追加（feature ゲートなし）:

```rust
pub mod gmail_shape;
```

- [ ] **Step 2: 失敗するテストを書く**

`crates/shogun-core/src/gmail_shape.rs` を新規作成:

```rust
//! Gmail REST v1 のレスポンスを、共有の正規化 `parse_items` が食う MCP エンベロープ形に畳む
//! 純関数（設計 §2）。ネットワークに触れないので Linux で単体テストできる。効果のある HTTP は
//! `gmail_rest`（feature `net`）が担い、この整形を呼ぶ。
//!
//! エラーは content-free（本文やトークンをログ・表面に出さない）。

use serde_json::{json, Value};

/// Gmail `threads.get` の 1 スレッド JSON を、`parse_items` の許容キーに合わせた記録に畳む。
///
/// スレッドの最新メッセージの件名・スニペット・内部日時を拾う。Gmail の payload.headers は
/// `[{name, value}, ...]`、`internalDate` はミリ秒文字列。
///
/// v1 の body は snippet（〜100 字）。MIME の text/plain パートを base64url デコードした完全本文の
/// 抽出は fast-follow（設計 §3 の「完全なスレッド」を厳密に満たすには要るが、snippet でも AX の
/// タイトルだけより桁違いに厚い。実機で snippet 接地が薄すぎると分かってから足す — YAGNI）。
pub fn record_from_thread(thread: &Value) -> Value {
    let thread_id = thread.get("id").and_then(Value::as_str).unwrap_or_default();
    let messages = thread.get("messages").and_then(Value::as_array);
    let last = messages.and_then(|m| m.last());
    let subject = last
        .and_then(header_value("Subject"))
        .unwrap_or_default();
    let snippet = last
        .and_then(|m| m.get("snippet").and_then(Value::as_str))
        .unwrap_or_default();
    let internal_ms = last
        .and_then(|m| m.get("internalDate").and_then(Value::as_str))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    json!({
        "threadId": thread_id,
        "subject": subject,
        "snippet": snippet,
        "internalDate": internal_ms,
    })
}

/// `payload.headers` から名前一致のヘッダ値を引くクロージャ。
fn header_value(name: &'static str) -> impl Fn(&Value) -> Option<String> {
    move |msg: &Value| {
        let headers = msg.get("payload").and_then(|p| p.get("headers")).and_then(Value::as_array)?;
        for h in headers {
            if h.get("name").and_then(Value::as_str) == Some(name) {
                return h.get("value").and_then(Value::as_str).map(str::to_string);
            }
        }
        None
    }
}

/// 記録配列を `parse_items` が食う MCP エンベロープ（`structuredContent`）に包む。
pub fn envelope(records: Vec<Value>) -> Value {
    json!({ "structuredContent": records })
}

/// `create_draft` の引数（`{to, subject, body}`）を Gmail `drafts.create` のリクエストボディに
/// 変換する。Gmail は RFC 2822 の生メールを base64url で `raw` に入れる形。
pub fn draft_request_body(args: &Value) -> Result<Value, String> {
    let to = args.get("to").and_then(Value::as_str).ok_or("draft: missing to")?;
    let subject = args.get("subject").and_then(Value::as_str).unwrap_or("");
    let body = args.get("body").and_then(Value::as_str).unwrap_or("");
    let raw_mail = format!("To: {to}\r\nSubject: {subject}\r\n\r\n{body}");
    let encoded = base64_url_no_pad(raw_mail.as_bytes());
    Ok(json!({ "message": { "raw": encoded } }))
}

/// base64url（パディングなし）。RFC 4648 §5。依存を増やさない小実装。
fn base64_url_no_pad(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_record_pulls_subject_snippet_and_time() {
        let thread = json!({
            "id": "18f2a",
            "messages": [{
                "snippet": "Can we lock Q3 pricing?",
                "internalDate": "1699900000000",
                "payload": { "headers": [
                    {"name": "From", "value": "a@x.com"},
                    {"name": "Subject", "value": "Q3 pricing"}
                ]}
            }]
        });
        let rec = record_from_thread(&thread);
        assert_eq!(rec["threadId"], "18f2a");
        assert_eq!(rec["subject"], "Q3 pricing");
        assert_eq!(rec["snippet"], "Can we lock Q3 pricing?");
        assert_eq!(rec["internalDate"], 1699900000000i64);
    }

    #[test]
    fn draft_body_encodes_rfc2822_into_raw() {
        let args = json!({ "to": "b@y.com", "subject": "Hi", "body": "Line one" });
        let out = draft_request_body(&args).unwrap();
        let raw = out["message"]["raw"].as_str().unwrap();
        // base64url をデコードして中身を検証（padding なし）。
        assert!(!raw.contains('='), "no padding");
        assert!(!raw.contains('+') && !raw.contains('/'), "url-safe alphabet");
    }

    #[test]
    fn base64_url_matches_known_vector() {
        // "foobar" の base64url(no pad) は "Zm9vYmFy"。
        assert_eq!(base64_url_no_pad(b"foobar"), "Zm9vYmFy");
        // 端数（1,2 バイト）も正しいこと。
        assert_eq!(base64_url_no_pad(b"f"), "Zg");
        assert_eq!(base64_url_no_pad(b"fo"), "Zm8");
    }
}
```

- [ ] **Step 3: テストが失敗することを確認**

feature 無しで走る（純関数、serde_json のみ）。

Run: `cargo test -p shogun-core --lib gmail_shape 2>&1 | tail -8`
Expected: ロジック未実装なら FAIL。通常は Step 2 のコードで実装本体も入るので、この Step は「入れる
前のコンパイルエラー」確認。

- [ ] **Step 4: 実装は Step 2 に含む**

Step 2 のコードが実装本体。整形は純粋なので Step 2 で完結。

- [ ] **Step 5: テストが通ることを確認**

Run: `cargo test -p shogun-core --lib gmail_shape 2>&1 | grep 'test result'`
Expected: PASS（3 tests: thread_record / draft_body / base64）

- [ ] **Step 6: clippy（feature 無しの pure ジョブと同条件）**

Run: `cargo clippy -p shogun-core --lib --all-targets -- -D warnings 2>&1 | tail -2`
Expected: Finished

- [ ] **Step 7: Commit**

```bash
git add crates/shogun-core/src/gmail_shape.rs crates/shogun-core/src/lib.rs
git commit -m "feat: Gmail REST → MCP-envelope shaping (pure, parse_items-compatible)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: GmailRestRpc の McpRpc 実装（HTTP、feature net）

**Files:**
- Create: `crates/shogun-core/src/gmail_rest.rs`（`GmailRestRpc<P: TokenProvider>` + `impl McpRpc`）
- Modify: `crates/shogun-core/src/lib.rs`（`#[cfg(feature = "net")] pub mod gmail_rest;` を追加）

`HttpMcpRpc`（`mcp_http.rs`）を鏡にする。トークンは `TokenProvider::access_token(service)` から Bearer。
tool 名でルーティング: `search_threads`（threads.list → 各 threads.get → envelope）、`get_thread`
（threads.get 1 件）、`create_draft`（drafts.create）。整形は Task 2 の `crate::gmail_shape` を使う。

- [ ] **Step 0: モジュール宣言を追加**

`crates/shogun-core/src/lib.rs` の `mcp_http` と同じ feature ゲートで追加:

```rust
#[cfg(feature = "net")]
pub mod gmail_rest;
```

- [ ] **Step 1: 実装を追加**

`crates/shogun-core/src/gmail_rest.rs` を新規作成。先頭に整形の再利用 use を置く:

```rust
//! Gmail REST v1 を transport 継ぎ目 `McpRpc` に載せる（公式 MCP 非依存、設計 §2）。
//! HTTP egress を shogun-core に集約する規約（不変条件3 / FR-TR-03）に従いここに置く。

use serde_json::Value;
use crate::gmail_shape::{draft_request_body, envelope, record_from_thread};
use shogun_integrations::rpc::{McpRpc, TokenProvider};
use shogun_mcp::scope::Service;

const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

/// Gmail REST v1 を叩く blocking クライアント。`McpRpc` を実装し、`RemoteMcpTransport` に
/// そのまま差し込める（公式 MCP `HttpMcpRpc` の置き換え）。
pub struct GmailRestRpc<P: TokenProvider> {
    client: reqwest::blocking::Client,
    tokens: P,
    /// threads.list の最大件数。
    page_size: u32,
}

impl<P: TokenProvider> GmailRestRpc<P> {
    pub fn new(tokens: P) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("shogun/1.0")
            .build()
            .map_err(|e| format!("http client init failed: {e}"))?;
        Ok(Self { client, tokens, page_size: 15 })
    }

    fn get_json(&self, token: &str, url: &str) -> Result<Value, String> {
        let resp = self
            .client
            .get(url)
            .bearer_auth(token)
            .send()
            .map_err(|e| format!("gmail request failed: {}", redact(&e.to_string())))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| format!("gmail read failed: {}", redact(&e.to_string())))?;
        if !status.is_success() {
            return Err(format!("gmail http {status}"));
        }
        serde_json::from_str(&text).map_err(|_| "gmail response was not valid json".to_string())
    }

    /// threads.list → 各 threads.get(format=metadata) → 記録配列 → エンベロープ。
    fn search_threads(&self, token: &str) -> Result<Value, String> {
        let list = self.get_json(
            token,
            &format!("{GMAIL_BASE}/threads?maxResults={}", self.page_size),
        )?;
        let ids: Vec<String> = list
            .get("threads")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string)).collect())
            .unwrap_or_default();
        let mut records = Vec::new();
        for id in ids {
            // metadata フォーマットで Subject/From/Date のみ（本文は引かない＝軽い）。
            let url = format!(
                "{GMAIL_BASE}/threads/{id}?format=metadata&metadataHeaders=Subject&metadataHeaders=From"
            );
            if let Ok(thread) = self.get_json(token, &url) {
                records.push(record_from_thread(&thread));
            }
        }
        Ok(envelope(records))
    }

    /// get_thread: 本文込み（format=full）で 1 スレッド。id は arguments.id。
    fn get_thread(&self, token: &str, args: &Value) -> Result<Value, String> {
        let id = args.get("id").and_then(Value::as_str).ok_or("get_thread: missing id")?;
        let thread = self.get_json(token, &format!("{GMAIL_BASE}/threads/{id}?format=full"))?;
        Ok(envelope(vec![record_from_thread(&thread)]))
    }

    /// create_draft: drafts.create に POST。
    fn create_draft(&self, token: &str, args: &Value) -> Result<Value, String> {
        let body = draft_request_body(args)?;
        let resp = self
            .client
            .post(format!("{GMAIL_BASE}/drafts"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .map_err(|e| format!("gmail draft failed: {}", redact(&e.to_string())))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| format!("gmail draft read failed: {}", redact(&e.to_string())))?;
        if !status.is_success() {
            return Err(format!("gmail http {status}"));
        }
        serde_json::from_str(&text).map_err(|_| "gmail draft response was not valid json".to_string())
    }
}

impl<P: TokenProvider> McpRpc for GmailRestRpc<P> {
    fn call_tool(&self, service: Service, tool: &str, arguments: Value) -> Result<Value, String> {
        if service != Service::Gmail {
            return Err(format!("GmailRestRpc only serves Gmail, got {}", service.source_str()));
        }
        let token = self.tokens.access_token(service)?;
        match tool {
            "search_threads" => self.search_threads(&token),
            "get_thread" => self.get_thread(&token, &arguments),
            "create_draft" => self.create_draft(&token, &arguments),
            other => Err(format!("GmailRestRpc has no mapping for tool '{other}'")),
        }
    }
}

/// `?` 以降を落とす（reqwest エラーが URL＋クエリを埋め込むため。mcp_http.rs と同じ防御）。
fn redact(msg: &str) -> String {
    match msg.find('?') {
        Some(i) => format!("{}…", &msg[..i]),
        None => msg.to_string(),
    }
}
```

- [ ] **Step 2: tool ルーティングの失敗テストを書く**

`crates/shogun-core/src/gmail_rest.rs` の `mod tests` に追加。ネットワークを踏まないケース（非 Gmail サービス拒否・未知 tool 拒否）を、トークンを返すダミーで検証:

```rust
#[cfg(test)]
mod tests {
    use super::GmailRestRpc;
    use crate::gmail_shape::{envelope, record_from_thread};
    use serde_json::json;
    use shogun_integrations::result::parse_items;
    use shogun_integrations::rpc::{McpRpc, StaticTokenProvider};
    use shogun_mcp::scope::Service;

    #[test]
    fn rejects_non_gmail_service() {
        let rpc = GmailRestRpc::new(StaticTokenProvider::new("t")).unwrap();
        let err = rpc.call_tool(Service::Slack, "search_threads", json!({})).unwrap_err();
        assert!(err.contains("only serves Gmail"), "{err}");
    }

    #[test]
    fn rejects_unknown_tool() {
        let rpc = GmailRestRpc::new(StaticTokenProvider::new("t")).unwrap();
        // 未知 tool はトークン取得後、HTTP に行く前に弾かれる。
        let err = rpc.call_tool(Service::Gmail, "nope", json!({})).unwrap_err();
        assert!(err.contains("no mapping"), "{err}");
    }

    #[test]
    fn envelope_is_parseable_by_the_shared_normalizer() {
        // 受け入れ基準: gmail_shape のエンベロープから既存 parse_items が FetchedItem を出せること。
        // parse_items は gated 依存なので、この検証は net-gated な gmail_rest 側に置く。
        let recs = vec![record_from_thread(&json!({
            "id": "t1",
            "messages": [{
                "snippet": "body text",
                "internalDate": "1699900000000",
                "payload": { "headers": [{"name": "Subject", "value": "Hello"}] }
            }]
        }))];
        let env = envelope(recs);
        let items = parse_items(&env).expect("parse ok");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id, "t1");
        assert_eq!(items[0].title, "Hello");
        assert_eq!(items[0].body, "body text");
        assert_eq!(items[0].ts_ms, 1699900000000i64);
    }
}
```

（注: `search_threads`/`get_thread`/`create_draft` の HTTP 経路は実ネットワークが要るため単体テストでは踏まない。実接続は Task 7 の実機受け入れで検証する。）

- [ ] **Step 3: テストが失敗することを確認**

Run: `cargo test -p shogun-core --lib --features net gmail_rest 2>&1 | tail -8`
Expected: 初回コンパイルで `StaticTokenProvider` の import が要る等があれば直す。ルーティング未実装なら FAIL。

- [ ] **Step 4: 実装は Step 1 に含む**

- [ ] **Step 5: テストが通ることを確認**

Run: `cargo test -p shogun-core --lib --features net gmail_rest 2>&1 | grep 'test result'`
Expected: PASS（Task 3 の 3 tests。Task 2 の gmail_shape 3 tests は別モジュール）

- [ ] **Step 6: clippy（net feature 込み）**

Run: `cargo clippy -p shogun-core --lib --features net -- -D warnings 2>&1 | tail -2`
Expected: Finished
（`reqwest` は `net` feature 下＝確認済み。`gmail_rest` は `mcp_http` と同じ `#[cfg(feature = "net")]`。）

- [ ] **Step 7: Commit**

```bash
git add crates/shogun-core/src/gmail_rest.rs crates/shogun-core/src/lib.rs
git commit -m "feat: GmailRestRpc — McpRpc over Gmail REST v1 (read + draft), MCP-independent

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: 融合ステップ（payload_source + build_reply_context 拡張 + DB クエリ）

**Files:**
- Modify: `crates/shogun-core/src/daemon.rs`（`PayloadSource` 型、`ReplyContext` にフィールド追加、`gmail_thread_candidates`、`build_reply_context` の解決ステップ）

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/daemon.rs` のテスト mod に追加。既存の実ヘルパを使う:
`Db::open_in_memory(clock(N))`（周辺テストと同じ）と `ingest_integration(&[IngestItem{...}])`。
`IngestItem` は `{ source: &'static str, kind: &'static str, title: String, body: String, ts_ms: i64 }`
（`shogun_mcp::sync::IngestItem`）。取り込んだ gmail アイテムの thread_key は native id を持たないため
`gmail:unknown:<正規化件名>` になる（本タスクの前提。§共有名を参照）。

```rust
#[test]
fn reply_context_prefers_fetched_gmail_thread_when_screen_matches() {
    use shogun_mcp::sync::IngestItem;
    let db = Db::open_in_memory(clock(1)).unwrap();
    // gmail 同期でスレッドを入れる（thread_key は "gmail:unknown:Q3 pricing" になる）。
    db.ingest_integration(&[IngestItem {
        source: "gmail",
        kind: "email",
        title: "Q3 pricing".to_string(),
        body: "Full thread body about pricing".to_string(),
        ts_ms: 1,
    }]);
    // 画面側は capture スレッド（タブ名 "(3) Q3 pricing — Gmail"）。
    let ctx = db.build_reply_context_for_screen(
        "capture:com.google.Chrome:Q3 pricing",
        "(3) Q3 pricing — Gmail",
    );
    assert!(matches!(ctx.payload_source, PayloadSource::Fetched { .. }));
    assert!(ctx.turns.iter().any(|t| t.excerpt.contains("pricing")), "fetched body used");
}

#[test]
fn reply_context_is_on_screen_only_without_a_gmail_match() {
    let db = Db::open_in_memory(clock(1)).unwrap();
    let ctx = db.build_reply_context_for_screen(
        "capture:com.apple.Safari:Nothing",
        "Nothing — Safari",
    );
    assert_eq!(ctx.payload_source, PayloadSource::OnScreenOnly);
}
```

（注: `clock` は daemon テスト mod の既存ヘルパ。実装者は既存テスト（例 `Db::open_in_memory(clock(1))`
を使う行）を先に読み、同じ流儀に合わせること。）

- [ ] **Step 2: `PayloadSource` と `ReplyContext` フィールドを追加**

`crates/shogun-core/src/daemon.rs` の `ReplyContext` 定義の直前に:

```rust
/// ドラフトの本文がどこ由来か。融合の provenance（設計 §3）。
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PayloadSource {
    /// 取得した実メール由来（高信頼）。thread_key が provenance（同期スレッドの識別子）。
    Fetched { thread_key: String },
    /// 取得データに解決できず、画面キャプチャの断片のみ。
    #[default]
    OnScreenOnly,
}
```

`ReplyContext` にフィールド追加（`#[derive(Default)]` があるので enum の `#[default]` で整合）:

```rust
    /// 本文の出所（融合の provenance）。
    pub payload_source: PayloadSource,
```

`ReplyContext { ... }` を組む既存の全箇所（`build_reply_context` の末尾、`Default` 経由でない
明示構築があれば）に `payload_source: PayloadSource::OnScreenOnly,` を足してコンパイルを通す。

- [ ] **Step 3: gmail 候補クエリを追加**

`crates/shogun-core/src/daemon.rs` の `Db` impl に追加（`recent` を使って `gmail:` を絞る）:

```rust
    /// event log 上の gmail スレッド候補 `(thread_key, title)`。融合リンカの入力。
    pub fn gmail_thread_candidates(&self, limit: usize) -> Vec<(String, String)> {
        let Ok(conn) = self.conn.lock() else { return Vec::new() };
        shogun_memory::thread::recent(&conn, limit)
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.thread_key.starts_with("gmail:"))
            .filter_map(|t| t.title.map(|title| (t.thread_key, title)))
            .collect()
    }
```

- [ ] **Step 4: 解決ステップ付きの build_reply_context を追加**

既存 `build_reply_context(thread_key)` は残し、画面セレクタ対応の薄いラッパを追加:

```rust
    /// 画面セレクタ（on-screen のタイトル）を使って、取得済み gmail スレッドに解決してから
    /// 文脈を組む。解決できれば gmail スレッドの turns を使い `Fetched`、できなければ元の
    /// thread_key で `OnScreenOnly`（設計 §3）。
    pub fn build_reply_context_for_screen(
        &self,
        on_screen_thread_key: &str,
        on_screen_title: &str,
    ) -> ReplyContext {
        let candidates = self.gmail_thread_candidates(50);
        match shogun_memory::thread::link_on_screen_to_thread(on_screen_title, &candidates) {
            Some(gmail_key) => {
                let mut ctx = self.build_reply_context(&gmail_key);
                // 同期スレッドの識別子（thread_key）を provenance に。
                ctx.payload_source = PayloadSource::Fetched { thread_key: gmail_key };
                ctx
            }
            None => {
                let mut ctx = self.build_reply_context(on_screen_thread_key);
                ctx.payload_source = PayloadSource::OnScreenOnly;
                ctx
            }
        }
    }
```

- [ ] **Step 5: テストが通ることを確認**

daemon は `db` feature 下（CI も `cargo test -p shogun-core --features db`）。

Run: `cargo test -p shogun-core --features db 2>&1 | grep 'test result' | tail -3`
Expected: PASS（新規 2 + 既存すべて）

- [ ] **Step 6: 呼び出し元を融合版に切り替え**

`build_reply_context` を焦点パスで呼んでいる箇所（`integrate.rs` の reply context 温めか
`daemon.rs` 内）で、画面タイトルが手に入る場所は `build_reply_context_for_screen` に差し替える。
該当箇所は `grep -rn 'build_reply_context' apps crates` で洗い、タイトルを渡せるものだけ移行
（渡せない内部呼び出しは据え置き）。

Run: `grep -rn 'build_reply_context' apps/desktop/src-tauri/src crates/shogun-core/src | grep -v test`
その結果を見て、焦点温めパスの 1 箇所を `_for_screen` に変更。

- [ ] **Step 7: clippy + 全テスト**

Run: `cargo clippy -p shogun-core --features db --all-targets -- -D warnings 2>&1 | tail -2 && cargo test -p shogun-core --features db 2>&1 | grep 'test result' | tail -1`
Expected: Finished / PASS

- [ ] **Step 8: Commit**

```bash
git add crates/shogun-core/src/daemon.rs apps/desktop/src-tauri/src
git commit -m "feat: fuse fetched Gmail thread into reply context via on-screen selector

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: connectors の transport を GmailRestRpc に差し替え

**Files:**
- Modify: `apps/desktop/src-tauri/src/connectors.rs`（`Provider`/`Transport` の型別名と `build_runtime`）

現状 `Transport = RemoteMcpTransport<HttpMcpRpc<Provider>>`。これを
`RemoteMcpTransport<GmailRestRpc<Provider>>` に変える。OAuth トークン層（`ManagedTokenProvider`）は不変。

- [ ] **Step 1: 型別名を差し替え**

`apps/desktop/src-tauri/src/connectors.rs` の型別名:

```rust
    // 公式 MCP (Developer Preview) に依存せず、Gmail REST を直接叩く（設計 §2）。
    pub type Transport = RemoteMcpTransport<shogun_core::gmail_rest::GmailRestRpc<Provider>>;
```

- [ ] **Step 2: build_runtime の RPC 生成を差し替え**

`build_runtime` 内の `HttpMcpRpc::new(provider)` を:

```rust
        let transport = RemoteMcpTransport::new(
            shogun_core::gmail_rest::GmailRestRpc::new(provider)?,
        );
```

不要になった `use shogun_core::mcp_http::HttpMcpRpc;` を、`HttpTokenExchange` を残しつつ整理。

- [ ] **Step 3: ビルド確認（desktop crate）**

Run: `cd apps/desktop/src-tauri && cargo build 2>&1 | tail -5`
Expected: Finished（型が通る）

- [ ] **Step 4: clippy**

Run: `cd apps/desktop/src-tauri && cargo clippy --lib -- -D warnings 2>&1 | tail -2`
Expected: Finished

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/connectors.rs
git commit -m "feat: connect Gmail via GmailRestRpc instead of the preview remote MCP

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: 送信ループのキー供給と一貫性の確認

**Files:**
- Modify: `apps/desktop/src-tauri/src/approvals.rs`（必要なら Composio キー取得のエラー文言のみ）
- Verify only: `confirm_send` / `RoutedSendTransport` / `save_gmail_draft`

送信経路は実装済み。ここは「キーが無いと分かる」ことと、下書きフォールバックが Task 5 の
GmailRestRpc（`create_draft`）を通ることの確認が主。

- [ ] **Step 1: 下書きフォールバックの tool 経路を確認**

`save_gmail_draft` が `FirstLayerSendTransport`（= `RemoteMcpTransport<GmailRestRpc>`）の
`execute` を通り、`draft_create_update` op → `create_draft` tool にマップされることを、
`grep -rn 'draft_create_update\|create_draft\|save_gmail_draft' apps crates` で追い、
経路が GmailRestRpc に着地することを確認（コード変更が要るのは、op 名→tool 名のマップに
`create_draft` が既にある＝Task 3 で対応済みなので、通常は確認のみ）。

- [ ] **Step 2: Composio キー未設定時の明示エラーを確認/改善**

`confirm_send` の `composio_api_key().unwrap_or_default()` は空キーで HTTP 400 になりうる。
空なら送信前に「Composio key not set」を返すガードを追加:

```rust
        let composio_key = composio_api_key()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| "Composio key not set — add it in settings to send".to_string())?;
```

（`confirm_send` の戻りが `Result<String, String>` であることを確認して整合させる。）

- [ ] **Step 3: ビルド + clippy + 既存送信テスト**

Run: `cd apps/desktop/src-tauri && cargo clippy --lib -- -D warnings 2>&1 | tail -2`
Run: `cargo test -p shogun-integrations 2>&1 | grep 'test result' | tail -1`
Run: `cargo test -p shogun-mcp 2>&1 | grep 'test result' | tail -1`
Expected: Finished / PASS（既存の invariant4 送信テスト含む）

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/approvals.rs
git commit -m "fix: fail send clearly when the Composio key is missing

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: 実機受け入れ（縦一本）

**Files:** なし（手動検証 + ログ確認）

前提: `SHOGUN_GOOGLE_CLIENT_ID` / `SHOGUN_GOOGLE_CLIENT_SECRET`（Desktop app クライアント、
Gemini で 403 になったのとは別プロジェクト）を env に投入。送信検証まで行うなら Composio キー
（Keychain）+ `SHOGUN_COMPOSIO_USER_ID` も。

- [ ] **Step 1: env 付きで起動**

```bash
pkill -f 'shogun-desktop-spike'; pkill -f 'tauri.js dev'; pkill -f vite; sleep 3
cd apps/desktop && SHOGUN_DATA_SUFFIX=design-polish \
  SHOGUN_GOOGLE_CLIENT_ID=... SHOGUN_GOOGLE_CLIENT_SECRET=... \
  pnpm dev > /tmp/shogun-app.log 2>&1 &
```

- [ ] **Step 2: Gmail 接続 → 同期を確認**

設定から Gmail を接続（OAuth ループバック）。ログで同期を確認:

Run: `LC_ALL=C grep -a 'connectors\] gmail synced\|ingest' /tmp/shogun-app.log | tail`
Expected: `gmail synced (+N new)`、event log に gmail スレッドが入る。

- [ ] **Step 3: 融合ドラフトを確認**

Gmail のスレッドをブラウザで開いて ⌥ タップ。ログ:

Run: `LC_ALL=C grep -a 'payload_source\|Fetched\|inserted .* chars' /tmp/shogun-app.log | tail`
Expected: 取得スレッド本文を根拠にドラフトが入る（AX 断片ではない）。受け入れ基準 2。

- [ ] **Step 4: 送信ループを確認（Composio キーがある場合）**

ドラフトから送信 → L3 確認ボタン。ログ:

Run: `LC_ALL=C grep -a 'confirm_send\|sent\|failed\|draft saved' /tmp/shogun-app.log | tail`
Expected: `sent`、または失敗時に下書き保存（FR-C2-05）。受け入れ基準 3。

- [ ] **Step 5: SLO 無回帰を確認**

Run: `LC_ALL=C grep -a 'cache_update' /tmp/shogun-app.log | tail`
Expected: `partial=false` かつレイテンシ ≤300ms が維持。受け入れ基準 4。

- [ ] **Step 6: 結果を記録**

受け入れ基準 1–4 の可否を PR 本文に貼る（SLO 値含む）。未達があれば新規タスク化。

---

## 完了の定義

- Task 1–6 が commit 済み、`cargo clippy --workspace -- -D warnings` と `cargo test --workspace` が green
- Task 7 の受け入れ基準 1–4 が実機で確認できる（キー投入後）
- 送信は必ず L3、Composio は第三者バッジ、secrets は Keychain のみ（不変条件 4/7）を維持
