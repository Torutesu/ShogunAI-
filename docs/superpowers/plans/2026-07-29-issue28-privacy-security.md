# Issue #28 — Privacy & Security 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ユーザーの LLM Key 保護・データ削除（1h/24h/全）・ログPIIマスキング・学習非利用の明示を、既存の不変条件をUIとRustコアに露出/拡張/文書化する形で実装する。

**Architecture:** データの重心は Rust コア（`shogun-memory` / `shogun-core`）。削除・マスキングは Rust に置き、webview は Tauri command 経由で呼ぶだけ。既存パターン（`maintenance.rs` の `delete_all`、`redact.rs` の secret masking、`llm.json` 設定ストア、`inline_source::mac::*` command）を土台に増築。スキーマ変更なし。

**Tech Stack:** Rust (rusqlite, security-framework, serde) / Tauri v2 / React + TypeScript。テスト: `cargo test`（in-memory SQLite）、`tsc --noEmit`。

**設計仕様:** `docs/superpowers/specs/2026-07-29-issue28-privacy-security-design.md`

**ブランチ:** `feat/issue-28-privacy-security`（main 起点）

**実装順:** スライスC（redactor・独立）→ B（削除コア+コマンド）→ A（UI枠）→ D（トグル+ポリシー）→ E（文書+導線）

**共通コマンド:**
- Rust テスト: `cargo test -p shogun-memory`, `cargo test -p shogun-core`, `cargo test -p shogun-desktop-spike`
- Rust lint: `cargo clippy -p <crate> -- -D warnings`（clippy warnings deny / CLAUDE.md）
- 型チェック: `pnpm --filter shogun-desktop typecheck`（または `apps/desktop` で `pnpm typecheck`）

---

## 0. 準備

### Task 0: ブランチ作成

- [ ] **Step 1: main 起点でブランチを切る**

作業ツリーの issue-24 未コミット変更を退避したうえで（`git stash` 等、必要時のみ）:

```bash
cd /Users/torutano/ShogunAI-
git fetch origin
git switch -c feat/issue-28-privacy-security origin/main
```

Expected: `Switched to a new branch 'feat/issue-28-privacy-security'`

---

## スライス C — ログ / PII redactor

**File Structure:**
- Modify: `crates/shogun-memory/src/redact.rs` — 既存 DB redactor はそのまま。ログ専用 `redact_log()` を追加（email / URL / issuer-prefix 強制マスク）。
- Test: `crates/shogun-memory/src/redact.rs`（`#[cfg(test)] mod tests` に追記）

`redact_log()` を DB redactor と**別関数**にする理由（設計判断②）: DB 経路で email をマスクすると `people.emails` 等の正当なメモリが壊れる。ログ経路だけが email/URL をマスクしてよい。

### Task C1: `redact_log` の失敗テストを書く

- [ ] **Step 1: 失敗テストを追加**

`crates/shogun-memory/src/redact.rs` の `mod tests` 末尾（`several_secrets_in_one_blob_are_all_masked` の後）に追加:

```rust
    fn rl(s: &str) -> String {
        redact_log(s).into_owned()
    }

    #[test]
    fn log_redactor_masks_emails() {
        assert_eq!(rl("user alice@example.com logged in"), "user [redacted] logged in");
        assert_eq!(rl("to: bob.smith+tag@sub.example.co.jp done"), "to: [redacted] done");
    }

    #[test]
    fn log_redactor_masks_full_urls_including_query() {
        assert_eq!(
            rl("GET https://api.example.com/v1/x?token=abc123&u=alice now"),
            "GET [redacted] now",
        );
        assert_eq!(rl("open http://localhost:3000/cb?code=xyz"), "open [redacted]");
    }

    #[test]
    fn log_redactor_still_masks_issuer_keys_and_labels() {
        assert_eq!(rl("key sk-ant-api03-abcdefghijklmnop"), "key [redacted]");
        assert_eq!(rl("api_key: abcdefghijklmnopqrst"), "api_key: [redacted]");
    }

    #[test]
    fn log_redactor_leaves_ordinary_prose_untouched() {
        let s = "Expanding notch panel in 92ms; cache updated.";
        assert_eq!(rl(s), s);
    }

    #[test]
    fn log_redactor_preserves_multibyte_around_matches() {
        let got = rl("送信先 alice@example.com へ通知しました");
        assert!(got.starts_with("送信先 "), "{got}");
        assert!(got.ends_with(" へ通知しました"), "{got}");
        assert!(got.contains("[redacted]") && !got.contains("alice@example.com"), "{got}");
    }
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cargo test -p shogun-memory redact::tests::log_redactor -- --nocapture`
Expected: FAIL（`cannot find function redact_log`）でコンパイルエラー

- [ ] **Step 3: `redact_log` を実装**

`crates/shogun-memory/src/redact.rs` の `redact` 関数の直後（`might_contain_secret` の前）に追加:

```rust
/// Redact for the **log / error-report path** (design decision ②). In addition to the DB
/// redactor's issuer-prefix and labelled-value masking, this also masks whole email addresses and
/// full URLs (query string included). It must NOT be used on capture content bound for the DB:
/// `people.emails` and captured prose legitimately contain emails, and masking them there would
/// corrupt the memory the product is built on. Logs are diagnostic, not memory — there, an email
/// or a URL with a token in the query is pure exposure with no upside.
pub fn redact_log(text: &str) -> std::borrow::Cow<'_, str> {
    // First pass: emails and URLs (log-only). Then run the shared secret redactor over the result.
    let stage1 = mask_emails_and_urls(text);
    match redact(&stage1) {
        std::borrow::Cow::Borrowed(_) => match stage1 {
            // stage1 changed nothing and redact changed nothing → borrow the original.
            std::borrow::Cow::Borrowed(_) => std::borrow::Cow::Borrowed(text),
            std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s),
        },
        std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s),
    }
}

/// Mask email addresses and full URLs. Hand-rolled (no regex dep, matching this module's style).
fn mask_emails_and_urls(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('@') && !text.contains("://") {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        if !text.is_char_boundary(i) {
            out.push_str(&text[i..i + 1]);
            i += 1;
            continue;
        }
        let rest = &text[i..];
        // URL: scheme "://" then a run of URL-ish characters (query included).
        if let Some(scheme_len) = url_scheme_len(rest) {
            let url_len = scheme_len + run_len_url(&rest[scheme_len..]);
            out.push_str(MASK);
            i += url_len;
            continue;
        }
        // Email: back up over the local part already emitted, then mask local@domain.
        if rest.starts_with('@') {
            if let Some((local_len, domain_len)) = email_span(&out, &rest[1..]) {
                out.truncate(out.len() - local_len);
                out.push_str(MASK);
                i += 1 + domain_len; // consume '@' + domain
                continue;
            }
        }
        let ch_len = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&rest[..ch_len]);
        i += ch_len;
    }
    std::borrow::Cow::Owned(out)
}

/// Length of a `scheme://` prefix at the start of `s`, or `None`.
fn url_scheme_len(s: &str) -> Option<usize> {
    let idx = s.find("://")?;
    // scheme must be short and alphabetic (http, https, ftp, ...) and immediately at the start.
    if idx == 0 || idx > 10 || !s[..idx].bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    Some(idx + 3)
}

/// Length of the URL body (host/path/query) after the scheme. Stops at whitespace or quote.
fn run_len_url(s: &str) -> usize {
    s.char_indices()
        .find(|(_, c)| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')'))
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

/// Given the text already emitted (`emitted`) ending in an email local part, and the text after
/// the `@`, return `(local_part_byte_len, domain_byte_len)` when both sides look like an email.
fn email_span(emitted: &str, after_at: &str) -> Option<(usize, usize)> {
    // local part: trailing run of email-local chars in what we've emitted.
    let local: String = emitted
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'))
        .collect();
    if local.is_empty() {
        return None;
    }
    let local_len = local.len();
    // domain: run of domain chars, must contain at least one dot before a whitespace/end.
    let domain_len = after_at
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
        .map(|(idx, _)| idx)
        .unwrap_or(after_at.len());
    let domain = &after_at[..domain_len];
    if domain_len == 0 || !domain.contains('.') || domain.ends_with('.') {
        return None;
    }
    Some((local_len, domain_len))
}
```

- [ ] **Step 4: テスト通過を確認**

Run: `cargo test -p shogun-memory redact::tests::log_redactor -- --nocapture`
Expected: 5 tests PASS

- [ ] **Step 5: 既存 DB redactor 非破壊を確認**

Run: `cargo test -p shogun-memory redact`
Expected: 既存テスト（`ordinary_text_is_untouched_and_not_reallocated` 等）を含め ALL PASS

- [ ] **Step 6: clippy**

Run: `cargo clippy -p shogun-memory -- -D warnings`
Expected: no warnings

- [ ] **Step 7: コミット**

```bash
git add crates/shogun-memory/src/redact.rs
git commit -m "feat(privacy): add log-path redactor for emails and URLs (#28)"
```

### Task C2: Rust ログ境界に `redact_log` を適用（高感度が通る経路）

**Files:**
- Create: `crates/shogun-core/src/log_redact.rs` — `elog!` マクロ（redact してから eprintln）
- Modify: `crates/shogun-core/src/lib.rs` — `pub mod log_redact;`

- [ ] **Step 1: 失敗テストを書く**

`crates/shogun-core/src/log_redact.rs` を新規作成:

```rust
//! Diagnostic logging that redacts before it writes (design decision ② / CLAUDE.md: logs must not
//! carry keys, tokens, emails or full URLs). Use `elog!` instead of a bare `eprintln!` anywhere a
//! message could interpolate user- or provider-derived text.

/// Redact a log line via the shared log-path redactor.
pub fn scrub(line: &str) -> String {
    shogun_memory::redact::redact_log(line).into_owned()
}

/// `eprintln!`-shaped macro that scrubs the formatted line first.
#[macro_export]
macro_rules! elog {
    ($($arg:tt)*) => {
        eprintln!("{}", $crate::log_redact::scrub(&format!($($arg)*)))
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn scrub_masks_a_key_in_a_log_line() {
        let out = super::scrub("provider error for sk-ant-api03-abcdefghijklmnop");
        assert!(!out.contains("sk-ant-api03"), "{out}");
        assert!(out.contains("[redacted]"), "{out}");
    }

    #[test]
    fn scrub_masks_an_email_in_a_log_line() {
        let out = super::scrub("failed to notify alice@example.com");
        assert!(!out.contains("alice@example.com"), "{out}");
    }
}
```

- [ ] **Step 2: モジュール登録**

`crates/shogun-core/src/lib.rs` の他の `pub mod` 宣言と同じ並びに追加:

```rust
pub mod log_redact;
```

（`redact::redact_log` を参照するため、`shogun_memory` が `shogun-core` の依存にあることを確認。既存で `shogun_memory::...` を使用しているので追加不要。）

- [ ] **Step 3: テスト失敗→実装済みなので通過を確認**

Run: `cargo test -p shogun-core log_redact`
Expected: 2 tests PASS

- [ ] **Step 4: LLM/送信経路の `eprintln!` を `elog!` に移行**

`crates/shogun-core/src/llm/anthropic.rs` 内で、レスポンス本文・エラー・キー由来の文字列を含みうる `eprintln!` を `elog!` に置換（`use crate::elog;` を追加、または `crate::elog!` で呼ぶ）。キーそのものは既に非ログだが、プロバイダのエラーボディに URL/メールが混じり得るため多層防御として適用する。

例（該当行の `eprintln!("...{err}...")` を）:

```rust
crate::elog!("[anthropic] request failed: {err}");
```

- [ ] **Step 5: テスト・clippy**

Run: `cargo test -p shogun-core && cargo clippy -p shogun-core -- -D warnings`
Expected: ALL PASS / no warnings

- [ ] **Step 6: コミット**

```bash
git add crates/shogun-core/src/log_redact.rs crates/shogun-core/src/lib.rs crates/shogun-core/src/llm/anthropic.rs
git commit -m "feat(privacy): scrub diagnostic logs via redact_log (#28)"
```

---

## スライス B — 時間範囲データ削除 + アカウント削除

**File Structure:**
- Modify: `crates/shogun-memory/src/maintenance.rs` — `delete_since()` を追加（`delete_all` と対称）
- Modify: `crates/shogun-core/src/daemon.rs` — `Db::delete_since()` を追加（`Db::delete_all` と対称）
- Modify: `apps/desktop/src-tauri/src/inline_source.rs` — `delete_data_since` / `delete_all_and_account` command
- Modify: `apps/desktop/src-tauri/src/lib.rs` — 新 command を `invoke_handler!` に登録

### Task B1: `maintenance::delete_since` の失敗テストを書く

- [ ] **Step 1: 失敗テストを追加**

`crates/shogun-memory/src/maintenance.rs` の `mod tests` 末尾に追加。`event_log.ts` が cutoff 以上のイベントと、それに紐づいて根拠を全て失う state を削除し、範囲外は残すことを検証:

```rust
    #[test]
    fn delete_since_removes_recent_events_and_keeps_older_ones() {
        let mut conn = crate::open_in_memory().unwrap();
        let old = insert_event(&conn, &NewEvent { ts: 1_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "old note", content_hash: "old",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let recent = insert_event(&conn, &NewEvent { ts: 9_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "recent note", content_hash: "new",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();

        let report = delete_since(&mut conn, 5_000).unwrap();
        assert_eq!(report.events, 1, "only the ts>=5000 event is deleted");

        let remaining: Vec<i64> = {
            let mut stmt = conn.prepare("SELECT id FROM event_log ORDER BY id").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
        };
        assert_eq!(remaining, vec![old], "old survives, recent gone");
        let _ = recent;
    }

    #[test]
    fn delete_since_drops_orphaned_state_but_keeps_still_supported_state() {
        let mut conn = crate::open_in_memory().unwrap();
        // A person supported by BOTH an old and a recent event survives; a commitment supported
        // ONLY by the recent event is orphaned and removed.
        let old = insert_event(&conn, &NewEvent { ts: 1_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "met Alice", content_hash: "e-old",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let recent = insert_event(&conn, &NewEvent { ts: 9_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: None, content: "Alice asked X", content_hash: "e-new",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        let alice = insert_person(&mut conn, &NewPerson { display_name: "Alice", confidence: 0.9, now: 1, ..Default::default() },
            &[Provenance::new(old), Provenance::new(recent)]).unwrap();
        insert_commitment(&mut conn, &NewCommitment { direction: CommitmentDirection::Mine,
            counterparty_id: Some(alice), description: "do X", due_at: None,
            status: CommitmentStatus::Open, project_id: None, confidence: 0.8, now: 1 },
            &[Provenance::new(recent)]).unwrap();

        delete_since(&mut conn, 5_000).unwrap();

        let people: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        let commitments: i64 = conn.query_row("SELECT count(*) FROM commitments", [], |r| r.get(0)).unwrap();
        assert_eq!(people, 1, "Alice keeps her old evidence, survives");
        assert_eq!(commitments, 0, "commitment lost all evidence, removed");
        // provenance pointing at the deleted event is gone; the old one remains.
        let prov: i64 = conn.query_row("SELECT count(*) FROM state_provenance", [], |r| r.get(0)).unwrap();
        assert_eq!(prov, 1, "only the old-event provenance row survives");
    }

    #[test]
    fn delete_since_keeps_the_schema_and_fts_in_sync() {
        let mut conn = crate::open_in_memory().unwrap();
        insert_event(&conn, &NewEvent { ts: 9_000, source: "capture", kind: "text",
            app_bundle_id: None, window_title: Some("Inbox"), content: "secret meeting", content_hash: "h",
            dwell_ms: 0, display_id: None, window_bounds: None }).unwrap();
        delete_since(&mut conn, 5_000).unwrap();
        // FTS mirror must not still return the deleted row.
        let hits: i64 = conn.query_row(
            "SELECT count(*) FROM event_fts WHERE event_fts MATCH 'secret'", [], |r| r.get(0)).unwrap();
        assert_eq!(hits, 0, "AD trigger cleared the FTS row");
    }
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cargo test -p shogun-memory maintenance::tests::delete_since`
Expected: FAIL（`cannot find function delete_since`）

- [ ] **Step 3: `delete_since` を実装**

`crates/shogun-memory/src/maintenance.rs` の `delete_all` の直後に追加:

```rust
/// Delete every user row whose occurrence time is at or after `cutoff_ts` (unix ms), and any state
/// row that loses ALL of its evidence as a result (design decision ③). Runs in a single
/// transaction. Derived summary text on a surviving state row may still reflect a deleted event
/// until the next Dream Cycle re-derivation — this is documented, not silently hidden.
pub fn delete_since(conn: &mut Connection, cutoff_ts: i64) -> Result<DeleteReport, rusqlite::Error> {
    let tx = conn.transaction()?;

    // Provenance rows that point at events we are about to delete go first (FK: they reference
    // event_log). This is what can orphan a state row.
    tx.execute(
        "DELETE FROM state_provenance WHERE event_id IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;

    // Vectors + cold embeddings for the doomed events (keyed on event id).
    tx.execute(
        "DELETE FROM event_vec WHERE rowid IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;
    tx.execute(
        "DELETE FROM cold_embeddings WHERE event_id IN (SELECT id FROM event_log WHERE ts >= ?1)",
        [cutoff_ts],
    )?;

    // The events themselves (AD trigger clears event_fts).
    let events = tx.execute("DELETE FROM event_log WHERE ts >= ?1", [cutoff_ts])?;

    // Meeting sessions started in the window, and their notes (notes reference sessions).
    let session_notes = tx.execute(
        "DELETE FROM session_notes WHERE session_id IN (SELECT id FROM sessions WHERE started_at >= ?1)",
        [cutoff_ts],
    )?;
    let sessions = tx.execute("DELETE FROM sessions WHERE started_at >= ?1", [cutoff_ts])?;

    // Traceability rows for sends in the window.
    let traceability = tx.execute("DELETE FROM traceability_log WHERE ts >= ?1", [cutoff_ts])?;

    // Orphan sweep: any state row with no surviving provenance is removed (children first).
    let commitments = tx.execute(orphan_sql("commitments"), [])?;
    let open_loops = tx.execute(orphan_sql("open_loops"), [])?;
    let people = tx.execute(orphan_sql("people"), [])?;
    let projects = tx.execute(orphan_sql("projects"), [])?;

    tx.commit()?;

    Ok(DeleteReport {
        events,
        people,
        projects,
        commitments,
        open_loops,
        threads: 0,
        sessions,
        session_notes,
        traceability,
    })
}

/// DELETE for one state table's rows that have no remaining provenance row.
fn orphan_sql(table: &'static str) -> &'static str {
    match table {
        "commitments" => "DELETE FROM commitments WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='commitments')",
        "open_loops" => "DELETE FROM open_loops WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='open_loops')",
        "people" => "DELETE FROM people WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='people')",
        "projects" => "DELETE FROM projects WHERE id NOT IN (SELECT state_id FROM state_provenance WHERE state_table='projects')",
        _ => unreachable!("unknown state table"),
    }
}
```

Note: `orphan_sql` は静的文字列 + 固定 `state_table` リテラルなので SQL インジェクション面は無い。`threads` は occurrence time を持たない派生キャッシュのため時間範囲削除の対象外（`delete_all` のみで消える）。

- [ ] **Step 4: テスト通過を確認**

Run: `cargo test -p shogun-memory maintenance::tests::delete_since`
Expected: 3 tests PASS

- [ ] **Step 5: 全 maintenance テスト + clippy**

Run: `cargo test -p shogun-memory maintenance && cargo clippy -p shogun-memory -- -D warnings`
Expected: ALL PASS（既存 `delete_all_*` 含む）/ no warnings

- [ ] **Step 6: コミット**

```bash
git add crates/shogun-memory/src/maintenance.rs
git commit -m "feat(privacy): add time-range deletion (delete_since) with orphan sweep (#28)"
```

### Task B2: `Db::delete_since` を追加

**Files:**
- Modify: `crates/shogun-core/src/daemon.rs`（`Db::delete_all` の直後）

- [ ] **Step 1: メソッド追加**

`crates/shogun-core/src/daemon.rs` の `delete_all`（737-740行付近）の直後に追加:

```rust
    /// Delete user data at or after `cutoff_ts` (unix ms), sweeping orphaned state (FR-SET-07 /
    /// #28). `None` on failure (the transaction leaves the DB untouched).
    pub fn delete_since(&self, cutoff_ts: i64) -> Option<shogun_memory::maintenance::DeleteReport> {
        let mut g = self.conn.lock().ok()?;
        shogun_memory::maintenance::delete_since(&mut g, cutoff_ts).ok()
    }
```

- [ ] **Step 2: ビルド確認**

Run: `cargo build -p shogun-core && cargo clippy -p shogun-core -- -D warnings`
Expected: builds / no warnings

- [ ] **Step 3: コミット**

```bash
git add crates/shogun-core/src/daemon.rs
git commit -m "feat(privacy): expose Db::delete_since (#28)"
```

### Task B3: Tauri command `delete_data_since` / `delete_all_and_account`

**Files:**
- Modify: `apps/desktop/src-tauri/src/inline_source.rs`（`clear_memory` 付近, 542行の後）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（`invoke_handler!`）

- [ ] **Step 1: command を追加**

`apps/desktop/src-tauri/src/inline_source.rs` の `clear_memory`（542行）の直後に追加。`Db` は `now_ms()` と `delete_since()`/`delete_all()` を持つ:

```rust
    /// Delete user data captured within the last `range` window (#28). `range` is "1h" or "24h".
    /// Local and immediate — nothing is sent anywhere. Returns the per-table deletion report.
    #[tauri::command]
    pub fn delete_data_since(range: String, db: tauri::State<'_, Db>) -> Result<String, String> {
        let window_ms: i64 = match range.as_str() {
            "1h" => 60 * 60 * 1000,
            "24h" => 24 * 60 * 60 * 1000,
            other => return Err(format!("unknown range: {other}")),
        };
        let cutoff = db.now_ms() - window_ms;
        let report = db.delete_since(cutoff).ok_or_else(|| "deletion failed".to_string())?;
        eprintln!("[shell] delete_data_since {range} — events={} people={} commitments={}",
            report.events, report.people, report.commitments);
        serde_json::to_string(&report).map_err(|e| e.to_string())
    }

    /// Delete ALL user data and every stored secret, then clear the account's local state (#28).
    /// Wipes the memory DB (schema kept) and removes every BYOK Keychain entry.
    #[tauri::command]
    pub fn delete_all_and_account(db: tauri::State<'_, Db>) -> Result<String, String> {
        let report = db.delete_all().ok_or_else(|| "deletion failed".to_string())?;
        // Remove every provider's BYOK key from the Keychain (best-effort; a missing key is fine).
        for provider in PROVIDERS {
            let _ = security_framework::passwords::delete_generic_password(
                KEYCHAIN_SERVICE, keychain_account(provider));
        }
        refresh_has_key();
        eprintln!("[shell] delete_all_and_account — all user data and BYOK keys removed");
        serde_json::to_string(&report).map_err(|e| e.to_string())
    }
```

Note: `DeleteReport` に `serde::Serialize` が要る。次ステップで derive を追加。

- [ ] **Step 2: `DeleteReport` を Serialize に**

`crates/shogun-memory/src/maintenance.rs` の `DeleteReport` の derive に `serde::Serialize` を追加:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DeleteReport {
```

（`shogun-memory` の Cargo.toml に `serde` が features=["derive"] で入っていることを確認。入っていなければ `serde = { workspace = true, features = ["derive"] }` を追加。）

- [ ] **Step 3: command を登録**

`apps/desktop/src-tauri/src/lib.rs` の `invoke_handler![...]` 内、`inline_source::mac::clear_memory` の隣に追加:

```rust
        inline_source::mac::delete_data_since,
        inline_source::mac::delete_all_and_account,
```

- [ ] **Step 4: ビルド + clippy**

Run: `cargo build -p shogun-desktop-spike && cargo clippy -p shogun-memory -- -D warnings`
Expected: builds / no warnings

- [ ] **Step 5: コミット**

```bash
git add apps/desktop/src-tauri/src/inline_source.rs apps/desktop/src-tauri/src/lib.rs crates/shogun-memory/src/maintenance.rs crates/shogun-memory/Cargo.toml
git commit -m "feat(privacy): add delete_data_since and delete_all_and_account commands (#28)"
```

---

## スライス A — 設定 UI「Privacy & Security」セクション

**File Structure:**
- Modify: `apps/desktop/src/App.tsx` — `<PrivacySecuritySection />` を新設し、既存 BYOK Key 入力（1572-1652）をこの中へ移設。既存セクション並び（1504-1508）に追加。
- Modify: `apps/desktop/src/strings.ts` — 文言追加

既存の `*Section` コンポーネント（`ApprovalsSection` 等）と、`clear_memory` の二段確認 UI（1366-1380）、BYOK 保存フロー（`saveKey`, 1416-1433）のパターンに合わせる。UI 文言は英語 + `strings.ts` 経由（i18n-ready、ブランドルール準拠：競合名・技術名を出さない、絵文字は ⚔ のみ）。

### Task A1: 文言を追加

- [ ] **Step 1: `strings.ts` に privacy 文言を追加**

`apps/desktop/src/strings.ts` の文言オブジェクトに追加（既存キー命名に合わせる）:

```ts
  privacyTitle: "Privacy & Security",
  keyEncryptedNote: "This key is encrypted in the macOS Keychain. No one — including our team — can read it in plaintext.",
  policyNotTrained: "Not used for model training",
  policyLocalFirst: "Local-first",
  policyEncrypted: "AES-256 / TLS 1.3",
  policyLink: "Read the full privacy policy",
  deleteTitle: "Delete data",
  deleteLast1h: "Last hour",
  deleteLast24h: "Last 24 hours",
  deleteAll: "Delete everything & account",
  deleteConfirm: "This can't be undone. Deleted from this device immediately.",
  deleteDone: "Deleted from this device.",
  analyticsTitle: "Anonymous usage stats",
  analyticsNote: "Off by default. When on, ShogunAI collects anonymous, aggregated stats to improve quality. Never your captured content.",
```

- [ ] **Step 2: 型チェック**

Run: `cd apps/desktop && pnpm typecheck`
Expected: no errors

- [ ] **Step 3: コミット**

```bash
git add apps/desktop/src/strings.ts
git commit -m "feat(privacy): add Privacy & Security UI strings (#28)"
```

### Task A2: `PrivacySecuritySection` コンポーネント（Key + ポリシーバッジ）

- [ ] **Step 1: コンポーネントを追加**

`apps/desktop/src/App.tsx` に新コンポーネントを追加（既存 `*Section` の定義位置に合わせる）。既存 BYOK Key 入力ロジック（`provider`, `keyInput`, `saveKey`, `keyState`）をこの中へ移設。設定済みは last-4 のみ表示（平文は取得しない）:

```tsx
function PrivacySecuritySection(): JSX.Element {
  // ...既存 BYOK state（provider/model/keyInput/keyState/keyMsg）をここへ移設...
  return (
    <section className="settings-section">
      <h3>{t.privacyTitle}</h3>

      {/* LLM API Key card */}
      <div className="card">
        {/* 既存の provider radio + model input + 非表示 key input + Save/Delete ボタン */}
        <p className="note">{t.keyEncryptedNote}</p>
      </div>

      {/* Data policy card */}
      <div className="card">
        <div className="badges">
          <span className="badge">{t.policyNotTrained}</span>
          <span className="badge">{t.policyLocalFirst}</span>
          <span className="badge">{t.policyEncrypted}</span>
        </div>
        <a href="https://shogunai.app/privacy" target="_blank" rel="noreferrer">{t.policyLink}</a>
      </div>

      {/* Data deletion card — Task B UI (A3) */}
      {/* Anonymous usage card — Task D UI (D2) */}
    </section>
  );
}
```

- [ ] **Step 2: セクションを設定パネルへ差し込む**

`App.tsx` の設定セクション並び（1504-1508 の `<ApprovalsSection />` 等の隣）に `<PrivacySecuritySection />` を追加。移設元の旧 BYOK ブロック（1572-1652）は削除し、重複を残さない（DRY）。

- [ ] **Step 3: 型チェック + ビルド**

Run: `cd apps/desktop && pnpm typecheck`
Expected: no errors

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src/App.tsx
git commit -m "feat(privacy): add Privacy & Security settings section with key + policy badges (#28)"
```

### Task A3: データ削除カード（1h / 24h / All + 確認 + トースト）

- [ ] **Step 1: 削除 UI を追加**

`PrivacySecuritySection` の "Data deletion card" に、3 ボタン + 確認ダイアログ + 実行トーストを実装。`All` は既存 clear memory 同様の二段確認（1366-1380 のパターン）:

```tsx
const [confirming, setConfirming] = useState<null | "1h" | "24h" | "all">(null);
const [deleteMsg, setDeleteMsg] = useState("");

const runDelete = (which: "1h" | "24h" | "all"): void => {
  if (!IN_TAURI) { setDeleteMsg(t.deleteDone); setConfirming(null); return; }
  const call = which === "all"
    ? invoke("delete_all_and_account")
    : invoke("delete_data_since", { range: which });
  void call.then(() => { setDeleteMsg(t.deleteDone); setConfirming(null); })
    .catch((e) => setDeleteMsg(String(e)));
};
```

ボタン群（`t.deleteLast1h` / `t.deleteLast24h` / `t.deleteAll`）を押すと `setConfirming(which)`、確認ダイアログに `t.deleteConfirm`、確定で `runDelete(which)`、結果は `deleteMsg` をトースト表示。

- [ ] **Step 2: 型チェック**

Run: `cd apps/desktop && pnpm typecheck`
Expected: no errors

- [ ] **Step 3: 手動確認（Tauri アプリ）**

Run: `cd apps/desktop && pnpm tauri dev`（または既存の dev 起動手順）
確認: 設定 → Privacy & Security → 「Last hour」で確認ダイアログ→確定→「Deleted from this device.」トースト。1h 実行後、直近1時間のキャプチャが検索に出ないこと。

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src/App.tsx
git commit -m "feat(privacy): add 1h/24h/all data deletion UI with confirm + toast (#28)"
```

---

## スライス D — 匿名統計トグル + ベンダーポリシー明示

**File Structure:**
- Modify: `apps/desktop/src-tauri/src/inline_source.rs` — `PrivacyPrefs` + `get_privacy_prefs`/`set_analytics_enabled` command + `analytics_enabled()` ゲート（既存 `LlmSettings`/`llm.json` パターンに準拠、`privacy.json` に保存）
- Modify: `apps/desktop/src-tauri/src/lib.rs` — command 登録
- Modify: `apps/desktop/src/App.tsx` — 匿名統計トグル UI
- Modify: `crates/shogun-core/src/llm/anthropic.rs` — ベンダーポリシーのコメント明記（判断①）

既定 **OFF（オプトイン）**（設計確定 §9-1）。#62 の PostHog 送信は必ず `analytics_enabled()` を通す契約点。

### Task D1: `PrivacyPrefs` ストア + ゲート + command

- [ ] **Step 1: ストアとゲート、command を追加**

`apps/desktop/src-tauri/src/inline_source.rs` の `LlmSettings` 群の近く（settings ストアの一群）に追加:

```rust
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct PrivacyPrefs {
        /// Anonymous, aggregated usage stats. OFF by default (opt-in) — #28 §9-1.
        pub analytics_enabled: bool,
    }
    impl Default for PrivacyPrefs {
        fn default() -> Self { Self { analytics_enabled: false } }
    }

    static PRIVACY_PREFS: std::sync::Mutex<Option<PrivacyPrefs>> = std::sync::Mutex::new(None);

    fn privacy_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        app.path().app_data_dir().ok().map(|d| d.join("privacy.json"))
    }

    /// Load persisted privacy prefs at setup (mirrors `init_llm_settings`).
    pub fn init_privacy_prefs(app: &tauri::AppHandle) {
        let mut p = PrivacyPrefs::default();
        if let Some(path) = privacy_path(app) {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(saved) = serde_json::from_str::<PrivacyPrefs>(&text) { p = saved; }
            }
        }
        if let Ok(mut g) = PRIVACY_PREFS.lock() { *g = Some(p); }
    }

    /// The one gate every analytics/telemetry send MUST pass through (#28, contract point for #62).
    pub fn analytics_enabled() -> bool {
        PRIVACY_PREFS.lock().ok().and_then(|g| g.clone()).unwrap_or_default().analytics_enabled
    }

    #[tauri::command]
    pub fn get_privacy_prefs() -> PrivacyPrefs {
        PRIVACY_PREFS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
    }

    #[tauri::command]
    pub fn set_analytics_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let p = PrivacyPrefs { analytics_enabled: enabled };
        if let Some(path) = privacy_path(&app) {
            if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
            let json = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
            std::fs::write(&path, json).map_err(|e| format!("save failed: {e}"))?;
        }
        if let Ok(mut g) = PRIVACY_PREFS.lock() { *g = Some(p); }
        eprintln!("[inline] analytics_enabled → {enabled}");
        Ok(())
    }
```

- [ ] **Step 2: setup で init を呼ぶ**

`apps/desktop/src-tauri/src/lib.rs` の setup 内、`init_llm_settings(...)` を呼んでいる箇所の隣に `inline_source::mac::init_privacy_prefs(&handle);` を追加（同じ AppHandle 引数）。

- [ ] **Step 3: command を登録**

`lib.rs` の `invoke_handler![...]` に追加:

```rust
        inline_source::mac::get_privacy_prefs,
        inline_source::mac::set_analytics_enabled,
```

- [ ] **Step 4: ビルド + clippy**

Run: `cargo build -p shogun-desktop-spike && cargo clippy -p shogun-desktop-spike -- -D warnings`
Expected: builds / no warnings

- [ ] **Step 5: コミット**

```bash
git add apps/desktop/src-tauri/src/inline_source.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(privacy): add opt-in analytics preference + analytics_enabled gate (#28)"
```

### Task D2: 匿名統計トグル UI

- [ ] **Step 1: トグルを追加**

`PrivacySecuritySection` の "Anonymous usage card" に実装:

```tsx
const [analytics, setAnalytics] = useState(false);
useEffect(() => {
  if (!IN_TAURI) return;
  void invoke<{ analytics_enabled: boolean }>("get_privacy_prefs")
    .then((p) => setAnalytics(p.analytics_enabled));
}, []);
const toggleAnalytics = (v: boolean): void => {
  setAnalytics(v);
  if (IN_TAURI) void invoke("set_analytics_enabled", { enabled: v }).catch(() => setAnalytics(!v));
};
```

カードに `t.analyticsTitle` / `t.analyticsNote` とトグル（`analytics` / `toggleAnalytics`）を配置。

- [ ] **Step 2: 型チェック + 手動確認**

Run: `cd apps/desktop && pnpm typecheck`
確認: トグル ON→OFF が `privacy.json` に永続化され、再起動後も保持される。既定は OFF。

- [ ] **Step 3: コミット**

```bash
git add apps/desktop/src/App.tsx
git commit -m "feat(privacy): add opt-in anonymous usage toggle UI (#28)"
```

### Task D3: ベンダーポリシーを正確に明記（判断①）

- [ ] **Step 1: Anthropic クライアントにポリシーコメントを追加**

`crates/shogun-core/src/llm/anthropic.rs` のクライアント定義の doc コメントに、事実ベースの記述を追加（コードの挙動は変えない — ヘッダは存在しないため）:

```rust
//! Data-use note (#28, design decision ①): the Anthropic API does not use inputs/outputs for
//! model training by default (Anthropic Commercial Terms). There is no per-request "do not train"
//! header; Zero Data Retention is an enterprise account-level setting, not a header. The
//! user-facing policy card and docs/privacy-security.md state this accurately; we set no header.
```

- [ ] **Step 2: ビルド確認**

Run: `cargo build -p shogun-core`
Expected: builds

- [ ] **Step 3: コミット**

```bash
git add crates/shogun-core/src/llm/anthropic.rs
git commit -m "docs(privacy): document Anthropic default no-training policy in client (#28)"
```

---

## スライス E — Privacy & Security 文書 + ローカル/クラウド境界

**File Structure:**
- Create: `docs/privacy-security.md` — 正典。何を/どこに/何に使い/何に使わないか、ローカル/クラウド境界表、削除ポリシー、学習非利用、将来のローカル限定モード境界定義。

### Task E1: `docs/privacy-security.md` を書く

- [ ] **Step 1: 文書を作成**

`docs/privacy-security.md` を新規作成。以下の節を含める（内容は本計画・仕様と整合）:

1. **What we store, and where** — データ種別（画面テキスト / 会議文字起こし / メタデータ / state）× 保存先（ローカル SQLite / macOS Keychain / クラウド送信の有無）の表。「画像・音声・録画は一切保存しない」（不変条件2）を明記。
2. **Local vs cloud boundary** — ローカル完結処理（キャプチャ・一次マスキング・軽量前処理・ローカル検索・オンデバイス ASR）vs クラウド送信処理（LLM プロンプト = 処理用チャンクのみ）の比較表。「ローカル優先」「標準」「将来のローカル限定モード（構想）」の3モード比較表。
3. **Not used for model training** — 判断①の正確な記述（Anthropic はデフォルト学習非利用 / 保持は悪用監視目的 / ZDR は将来）。プロバイダ別データ保持ポリシー表。
4. **Deleting your data** — 1h/24h/全削除 + アカウント削除の単位、ローカル即時削除であること、派生要約は次回 Dream Cycle まで影響が残り得る限界（判断③）を正直に記載。ベンダー側に送信済みデータの削除可否は「できない部分は正直に」記述。
5. **Anonymous usage stats** — 既定 OFF（オプトイン）、ON 時も匿名・集約のみでキャプチャ内容は含まない。
6. **Future: local-only mode** — 将来切り出しの境界定義（要件レベル）。

- [ ] **Step 2: ポリシーリンク先を確定**

Task A2 で使った `https://shogunai.app/privacy` の実 URL を確定（LP 側の該当ページ、または アプリ内文書へのローカルリンク）。未確定なら App.tsx のリンクを本 doc への相対導線に差し替え。

- [ ] **Step 3: コミット**

```bash
git add docs/privacy-security.md
git commit -m "docs(privacy): add Privacy & Security policy and local/cloud boundary (#28)"
```

### Task E2: オンボーディングにプライバシーステップ（issue-24 と調整）

- [ ] **Step 1: 3 行のプライバシーステップを追加**

進行中の issue-24 オンボーディング（`apps/desktop/src/onboarding/`）に、3 行 + 詳細リンクのステップを追加:
「Your key is encrypted in the macOS Keychain.」「Your data is never used for model training.」「You can delete everything anytime.」+ `t.policyLink`。

Note: issue-24 の作業ツリーと衝突しうるため、担当と調整のうえマージ順を決める。独立実装が難しければ本タスクは issue-24 側へ移譲。

- [ ] **Step 2: 型チェック**

Run: `cd apps/desktop && pnpm typecheck`
Expected: no errors

- [ ] **Step 3: コミット**

```bash
git add apps/desktop/src/onboarding/
git commit -m "feat(privacy): add privacy step to onboarding (#28)"
```

---

## 最終確認

### Task F: 全体テスト・lint・PR

- [ ] **Step 1: 全 Rust テスト**

Run: `cargo test -p shogun-memory && cargo test -p shogun-core && cargo test -p shogun-desktop-spike`
Expected: ALL PASS

- [ ] **Step 2: 全 clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 3: 型チェック**

Run: `pnpm typecheck`
Expected: no errors

- [ ] **Step 4: PR 作成（main base）**

```bash
git push -u origin feat/issue-28-privacy-security
gh pr create --base main --title "feat: Privacy & Security — key protection, data deletion, log masking (#28)" --body "Closes #28. スライス A–E: 設定UI Privacy&Security / 時間範囲削除(1h/24h/全)+アカウント削除 / ログPIIマスキング / 匿名統計オプトイン + ベンダーポリシー明示 / 文書。設計判断①学習オプトアウトは文書化・②ログ専用redactor・③孤児state削除。詳細は docs/superpowers/specs/2026-07-29-issue28-privacy-security-design.md"
```

---

## Self-Review（計画作成者チェック済み）

- **Spec coverage**: 仕様§5 の 5 スライス全てにタスクあり（A→A1-A3 / B→B1-B3 / C→C1-C2 / D→D1-D3 / E→E1-E2）。§3 の 3 判断はそれぞれ C（②）/ B1 Step3（③）/ D3・E1（①）に反映。§9 確定事項（トグル既定OFF・即時削除文言・main起点ブランチ）は D1/A1/Task0 に反映。
- **Placeholder scan**: コード変更ステップは全て実コード記載。UI（A2/A3/D2）は既存 `*Section`/`saveKey`/二段確認パターンを参照しつつ具体コードを提示。唯一の外部依存はポリシー URL（E2 Step2 で確定するタスク化済み）。
- **Type consistency**: `redact_log`（C1）→ `log_redact::scrub`/`elog!`（C2）、`delete_since`（B1）→ `Db::delete_since`（B2）→ `delete_data_since`（B3）、`PrivacyPrefs`/`analytics_enabled`（D1）→ `get_privacy_prefs`/`set_analytics_enabled`（D1/D2）、`DeleteReport` に `serde::Serialize`（B3 Step2）で command の JSON 返却と一致。
