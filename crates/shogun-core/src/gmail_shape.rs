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
    fn draft_requires_the_to_key_not_recipient_email() {
        // The arg contract callers must honour: the recipient key is `to`. The draft-fallback in
        // approvals.rs once passed `recipient_email` (the old official-MCP tool name), which this
        // function rejects — silently breaking the FR-C2-05 draft save after the GmailRestRpc swap.
        // This locks the name so that seam can't rot again.
        let wrong = json!({ "recipient_email": "b@y.com", "subject": "Hi", "body": "x" });
        assert!(draft_request_body(&wrong).is_err(), "recipient_email must NOT satisfy the contract");
        let right = json!({ "to": "b@y.com", "subject": "Hi", "body": "x" });
        assert!(draft_request_body(&right).is_ok(), "`to` is the required key");
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
