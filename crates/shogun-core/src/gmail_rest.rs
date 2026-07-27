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

    /// threads.list → 各 threads.get(format=full) → 記録配列 → エンベロープ。
    ///
    /// `format=full` にすることで `payload.parts`/`body.data` が含まれ、
    /// `record_from_thread` が text/plain 本文を base64url デコードして `body` フィールドに入れる。
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
            // format=full で payload.parts/body.data を含む完全なスレッドを取得する。
            let url = format!("{GMAIL_BASE}/threads/{id}?format=full");
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
