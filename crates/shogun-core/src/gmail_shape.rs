//! Gmail REST v1 のレスポンスを、共有の正規化 `parse_items` が食う MCP エンベロープ形に畳む
//! 純関数（設計 §2）。ネットワークに触れないので Linux で単体テストできる。効果のある HTTP は
//! `composio_read`（feature `net`）が担い、この整形を呼ぶ。
//!
//! エラーは content-free（本文やトークンをログ・表面に出さない）。

use serde_json::{json, Value};

/// スレッドの全メッセージから text/plain 本文を連結した文字列を返す。
///
/// 各メッセージの `payload` から `plain_body` を抽出し `"\n\n---\n\n"` で連結する。
/// 合計が 16 KB を超える場合は char 境界で切り詰めて `"…"` を付ける。
/// text/plain が一切ない場合は最後のメッセージの `snippet` を返す（フォールバック）。
pub fn thread_body(thread: &Value) -> String {
    const MAX_BYTES: usize = 16 * 1024;
    let messages = thread.get("messages").and_then(Value::as_array);
    let mut parts: Vec<String> = Vec::new();
    let mut last_snippet = String::new();
    if let Some(msgs) = messages {
        for msg in msgs {
            if let Some(s) = msg.get("snippet").and_then(Value::as_str) {
                last_snippet = s.to_string();
            }
            if let Some(payload) = msg.get("payload") {
                if let Some(body) = plain_body(payload) {
                    if !body.is_empty() {
                        parts.push(body);
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        return last_snippet;
    }
    let joined = parts.join("\n\n---\n\n");
    if joined.len() <= MAX_BYTES {
        return joined;
    }
    // char 境界で 16 KB に切り詰める。
    let mut end = MAX_BYTES;
    while !joined.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &joined[..end])
}

/// `payload` から text/plain 本文を抽出する（再帰、深さ優先）。
///
/// - `payload.mimeType == "text/plain"` かつ `payload.body.data` が非空 → base64url デコード。
/// - それ以外は `payload.parts[]` を再帰探索して最初に見つかった本文を返す。
pub fn plain_body(payload: &Value) -> Option<String> {
    let mime = payload.get("mimeType").and_then(Value::as_str).unwrap_or("");
    if mime == "text/plain" {
        if let Some(data) = payload
            .get("body")
            .and_then(|b| b.get("data"))
            .and_then(Value::as_str)
        {
            if !data.is_empty() {
                let bytes = base64_url_decode(data);
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
    }
    // parts を再帰探索（multipart/*)。
    if let Some(parts) = payload.get("parts").and_then(Value::as_array) {
        for part in parts {
            if let Some(found) = plain_body(part) {
                return Some(found);
            }
        }
    }
    None
}

/// base64url（パディングなし・行折り返し寛容）デコード。RFC 4648 §5。依存を増やさない小実装。
///
/// `-` → 62、`_` → 63。`=` と空白・改行は無視（Gmail が行折りすることがある）。
/// 不正な文字もスキップ（寛容）。
pub fn base64_url_decode(s: &str) -> Vec<u8> {
    // URL-safe alphabet のインデックスを返す。None = スキップ対象文字。
    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            b'=' | b' ' | b'\t' | b'\n' | b'\r' => None, // padding / whitespace: skip
            _ => None,                                     // 不正文字: skip（寛容）
        }
    }

    let mut out = Vec::with_capacity(s.len() * 3 / 4 + 2);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for byte in s.bytes() {
        if let Some(val) = decode_char(byte) {
            buf = (buf << 6) | (val as u32);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((buf >> bits) & 0xFF) as u8);
            }
        }
    }
    out
}

/// Gmail `threads.get` の 1 スレッド JSON を、`parse_items` の許容キーに合わせた記録に畳む。
///
/// スレッドの最新メッセージの件名・スニペット・内部日時を拾う。Gmail の payload.headers は
/// `[{name, value}, ...]`、`internalDate` はミリ秒文字列。
///
/// `body` フィールドに全メッセージの text/plain 本文を連結したものを入れる（`parse_items` が
/// `"body"` キーを `"snippet"` より優先するため、これが実際の本文として使われる）。
/// `snippet` フィールドも残す（ベルト＆サスペンダー）。
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
    let body = thread_body(thread);
    json!({
        "threadId": thread_id,
        "subject": subject,
        "snippet": snippet,
        "body": body,
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

/// base64url（パディングなし）。RFC 4648 §5。依存を増やさない小実装。
/// テスト専用ヘルパ: 本番の唯一の利用者だった Gmail REST 版 `draft_request_body` は、draft 作成が
/// Composio 経由（`composio_read::create_draft`）に一本化されたため削除済み。エンコーダはテストの
/// フィクスチャ生成にのみ使うので `#[cfg(test)]` に留める。
#[cfg(test)]
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

    // ── base64_url_decode ──────────────────────────────────────────────────────

    #[test]
    fn base64_url_decode_known_vectors() {
        // "foobar" の base64url は "Zm9vYmFy"。
        assert_eq!(base64_url_decode("Zm9vYmFy"), b"foobar");
        // 2 文字末尾 → 1 バイト。
        assert_eq!(base64_url_decode("Zg"), b"f");
        // 3 文字末尾 → 2 バイト。
        assert_eq!(base64_url_decode("Zm8"), b"fo");
    }

    #[test]
    fn base64_url_decode_tolerates_newline_inside() {
        // Gmail が行折りを入れる場合に寛容であることを確認。
        let with_newline = "Zm9v\nYmFy";
        assert_eq!(base64_url_decode(with_newline), b"foobar");
    }

    // ── plain_body ─────────────────────────────────────────────────────────────

    #[test]
    fn plain_body_single_part_text_plain() {
        // "hello" の base64url は "aGVsbG8"。
        let payload = json!({
            "mimeType": "text/plain",
            "body": { "data": "aGVsbG8" }
        });
        let result = plain_body(&payload).expect("should find body");
        assert_eq!(result, "hello");
    }

    #[test]
    fn plain_body_multipart_nested_text_plain() {
        // text/plain が parts 内にネストされているケース。
        let payload = json!({
            "mimeType": "multipart/alternative",
            "parts": [
                {
                    "mimeType": "text/html",
                    "body": { "data": "PGh0bWw-" }
                },
                {
                    "mimeType": "text/plain",
                    "body": { "data": "aGVsbG8" }
                }
            ]
        });
        let result = plain_body(&payload).expect("should find plain body in parts");
        assert_eq!(result, "hello");
    }

    #[test]
    fn plain_body_html_only_returns_none() {
        let payload = json!({
            "mimeType": "text/html",
            "body": { "data": "PGh0bWw-" }
        });
        assert!(plain_body(&payload).is_none(), "text/html のみは None");
    }

    // ── thread_body ────────────────────────────────────────────────────────────

    #[test]
    fn thread_body_joins_messages() {
        // "hello" = "aGVsbG8"、"world" = "d29ybGQ"。
        let thread = json!({
            "messages": [
                {
                    "snippet": "ignored",
                    "payload": {
                        "mimeType": "text/plain",
                        "body": { "data": "aGVsbG8" }
                    }
                },
                {
                    "snippet": "also ignored",
                    "payload": {
                        "mimeType": "text/plain",
                        "body": { "data": "d29ybGQ" }
                    }
                }
            ]
        });
        let body = thread_body(&thread);
        assert!(body.contains("hello"), "first message body present");
        assert!(body.contains("world"), "second message body present");
        assert!(body.contains("---"), "separator present");
    }

    #[test]
    fn thread_body_falls_back_to_snippet_when_no_plain_part() {
        let thread = json!({
            "messages": [{
                "snippet": "fallback snippet text",
                "payload": {
                    "mimeType": "text/html",
                    "body": { "data": "PGh0bWw-" }
                }
            }]
        });
        let body = thread_body(&thread);
        assert_eq!(body, "fallback snippet text");
    }

    #[test]
    fn thread_body_truncates_at_16kb() {
        // 17 KB のテキストを base64url エンコードして渡し、切り詰めを確認する。
        let long_text = "A".repeat(17 * 1024);
        let encoded = base64_url_no_pad(long_text.as_bytes());
        let thread = json!({
            "messages": [{
                "payload": {
                    "mimeType": "text/plain",
                    "body": { "data": encoded }
                }
            }]
        });
        let body = thread_body(&thread);
        assert!(body.len() <= 16 * 1024 + 4, "should be truncated (allow for ellipsis bytes)");
        assert!(body.ends_with('…'), "should end with ellipsis");
    }

    // ── record_from_thread ─────────────────────────────────────────────────────

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
    fn record_from_thread_has_decoded_body() {
        // "hello world" の base64url は "aGVsbG8gd29ybGQ"。
        let thread = json!({
            "id": "t42",
            "messages": [{
                "snippet": "short snippet",
                "internalDate": "1699900000000",
                "payload": {
                    "mimeType": "text/plain",
                    "body": { "data": "aGVsbG8gd29ybGQ" },
                    "headers": [{"name": "Subject", "value": "Test"}]
                }
            }]
        });
        let rec = record_from_thread(&thread);
        let body = rec["body"].as_str().expect("body field should be a string");
        assert!(body.contains("hello world"), "decoded body should contain the plain text");
        // snippet も残っていること（ベルト＆サスペンダー）。
        assert_eq!(rec["snippet"], "short snippet");
    }

    // ── base64_url_no_pad (test-only encoder helper) ───────────────────────────

    #[test]
    fn base64_url_matches_known_vector() {
        // "foobar" の base64url(no pad) は "Zm9vYmFy"。
        assert_eq!(base64_url_no_pad(b"foobar"), "Zm9vYmFy");
        // 端数（1,2 バイト）も正しいこと。
        assert_eq!(base64_url_no_pad(b"f"), "Zg");
        assert_eq!(base64_url_no_pad(b"fo"), "Zm8");
    }
}
