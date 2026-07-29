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
