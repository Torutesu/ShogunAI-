//! 届いた端からテキストを取り出すSSEデコーダ。
//!
//! [`super::anthropic::parse_sse_text`] はボディが揃ってから解析するので、最初の一文字が
//! 出るのは応答が終わったあと — 「初トークン1s」というSLOはそれでは測りようがない。この型は
//! 同じイベント形（`content_block_delta` の `text_delta`）を、チャンクが行の途中で切れても
//! 落とさずに読む。
//!
//! ネットワークには触らない純ロジックなので、feature gate も要らずLinuxでテストできる。

use serde_json::Value;

/// 到着順にSSEを読み、テキストデルタだけを吐くデコーダ。1本のストリームにつき1つ作る。
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// まだ行として完成していない末尾。次のチャンクの頭と繋がる。
    pending: String,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// ボディのチャンクを1つ食わせ、この時点で確定したテキストデルタを順番に返す。
    ///
    /// 行が完成していない部分は内部に持ち越す。チャンクが `"data: {\"ty"` で切れても、次の
    /// push で残りと連結してから解釈されるので、デルタは失われない。
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);
        let mut out = Vec::new();

        // 最後の改行までを行として処理し、その先は次回に持ち越す。改行が無ければ何もしない。
        let Some(cut) = self.pending.rfind('\n') else { return out };
        let complete: String = self.pending.drain(..=cut).collect();

        for line in complete.lines() {
            let line = line.trim_start();
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            // 壊れた1行でストリーム全体を落とさない。落とすのはその行だけ。
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if v.get("type").and_then(Value::as_str) != Some("content_block_delta") {
                continue;
            }
            let Some(delta) = v.get("delta") else { continue };
            if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
                continue;
            }
            if let Some(t) = delta.get("text").and_then(Value::as_str) {
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELTA_A: &str =
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n";
    const DELTA_B: &str =
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n";

    #[test]
    fn a_complete_event_yields_its_text() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(DELTA_A), vec!["Hel".to_string()]);
    }

    /// 一度の push に複数イベントが入っていても全部返す。
    #[test]
    fn several_events_in_one_chunk_all_come_out() {
        let mut d = SseDecoder::new();
        let both = format!("{DELTA_A}{DELTA_B}");
        assert_eq!(d.push(&both), vec!["Hel".to_string(), "lo".to_string()]);
    }

    /// これがこの型の存在理由: 行の途中で切れたチャンクを落とさない。
    #[test]
    fn an_event_split_across_chunks_is_not_lost() {
        let mut d = SseDecoder::new();
        let (head, tail) = DELTA_A.split_at(30);

        assert!(d.push(head).is_empty(), "行が完成する前に何か返している");
        assert_eq!(d.push(tail), vec!["Hel".to_string()]);
    }

    /// text_delta 以外のイベント（ping, message_start, content_block_stop …）は無視する。
    #[test]
    fn non_text_events_are_ignored() {
        let mut d = SseDecoder::new();
        let noise = "event: ping\ndata: {\"type\":\"ping\"}\n\n\
                     data: {\"type\":\"message_start\",\"message\":{}}\n\n";
        assert!(d.push(noise).is_empty());
    }

    #[test]
    fn the_done_sentinel_is_ignored() {
        let mut d = SseDecoder::new();
        assert!(d.push("data: [DONE]\n\n").is_empty());
    }

    /// 壊れたJSONで止まらない。1行落として次へ進む。
    #[test]
    fn malformed_json_does_not_stop_the_stream() {
        let mut d = SseDecoder::new();
        assert!(d.push("data: {not json\n\n").is_empty());
        assert_eq!(d.push(DELTA_A), vec!["Hel".to_string()]);
    }

    /// 既存の parse_sse_text と同じ結果に落ち着く。二つの実装が食い違わないことの確認。
    #[test]
    fn the_incremental_result_matches_the_whole_body_parser() {
        let body = format!("{DELTA_A}{DELTA_B}data: [DONE]\n\n");
        let mut d = SseDecoder::new();
        let incremental: String = d.push(&body).concat();

        assert_eq!(incremental, super::super::anthropic::parse_sse_text(&body));
    }
}
