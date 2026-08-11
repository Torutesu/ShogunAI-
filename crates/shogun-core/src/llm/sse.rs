//! 届いた端からテキストを取り出すSSEデコーダ。
//!
//! [`super::anthropic::parse_sse_text`] はボディが揃ってから解析するので、最初の一文字が
//! 出るのは応答が終わったあと — 「初トークン1s」というSLOはそれでは測りようがない。この型は
//! 同じイベント形を、チャンクが行の途中で切れても落とさずに読む。
//!
//! 対応する形は2つ（[`SseFlavor`]）。行の切り出し・持ち越し・壊れた行の握り潰しは両者で
//! 完全に同じで、違うのは「1つの `data:` JSONからテキストをどう取り出すか」だけ — なので
//! そこだけを分岐させ、面倒な側（carry）は1本に保つ。プロバイダごとにデコーダを丸ごと
//! 書くと、この carry のバグを2箇所で踏むことになる。
//!
//! ネットワークには触らない純ロジックなので、feature gate も要らずLinuxでテストできる。
//!
//! `data:` 行はそれぞれ独立した完結したJSONとして読む。SSE仕様が認める複数行 `data:` の
//! 連結は実装していない — プロバイダが実際に送ってくる形に合わせた意図的な割り切りで、
//! 既存の `parse_sse_text` も同じ読み方をしている。

use serde_json::Value;

/// どのプロバイダのイベント形を読むか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SseFlavor {
    /// Anthropic Messages API: `content_block_delta` の `delta.text_delta.text`。
    #[default]
    Anthropic,
    /// OpenAI互換 `chat/completions`: `choices[].delta.content`。
    /// OpenRouter / OpenAI / Gemini の互換面はすべてこの形。
    OpenAi,
}

impl SseFlavor {
    /// `data:` 1行ぶんのJSONから、このフレーバのテキストデルタを取り出す。
    ///
    /// 該当しないイベント（ping, message_start, tool 呼び出しのデルタ, usage-only の
    /// 最終チャンク…）では `None`。空文字も `None` に畳む — 呼び出し側は「返ってきた＝
    /// 画面に出す文字がある」とだけ考えればよくなる。
    fn text_of(self, v: &Value) -> Option<String> {
        match self {
            Self::Anthropic => {
                if v.get("type").and_then(Value::as_str)? != "content_block_delta" {
                    return None;
                }
                let delta = v.get("delta")?;
                if delta.get("type").and_then(Value::as_str)? != "text_delta" {
                    return None;
                }
                non_empty(delta.get("text")?.as_str()?)
            }
            // `choices` が空の配列で来るチャンクがある（プロンプトフィルタの結果など）ので、
            // 添字ではなく `first()` で取る。`content` が null のチャンク（finish_reason
            // だけを運ぶ最後の1つ）も同じ経路で素通りする。
            Self::OpenAi => {
                let choice = v.get("choices")?.as_array()?.first()?;
                non_empty(choice.get("delta")?.get("content")?.as_str()?)
            }
        }
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// 到着順にSSEを読み、テキストデルタだけを吐くデコーダ。1本のストリームにつき1つ作る。
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// まだ行として完成していない末尾。次のチャンクの頭と繋がる。
    pending: String,
    flavor: SseFlavor,
}

impl SseDecoder {
    /// Anthropic 形のデコーダ。
    pub fn new() -> Self {
        Self::default()
    }

    /// OpenAI互換形のデコーダ。
    pub fn openai() -> Self {
        Self { pending: String::new(), flavor: SseFlavor::OpenAi }
    }

    /// ボディのチャンクを1つ食わせ、この時点で確定したテキストデルタを順番に返す。
    ///
    /// 行が完成していない部分は内部に持ち越す。チャンクが `"data: {\"ty"` で切れても、次の
    /// push で残りと連結してから解釈されるので、デルタは失われない。
    ///
    /// 呼び出し側は有効なUTF-8を渡すこと。ネットワークのバイト列をそのまま `&str` にすると
    /// マルチバイト文字がチャンク境界で割れるので、途中で切れた文字のバイトを次のチャンクへ
    /// 持ち越すのはトランスポート側の責任（`llm::transport` の carry バッファ）。ここは
    /// 「行が途中で切れる」ことだけを引き受ける。
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
            if let Some(t) = self.flavor.text_of(&v) {
                out.push(t);
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

    /// JSONは揃っているのに改行がまだ来ていないチャンク。行が完成するまで出さない、という
    /// 判断がここで効く（byte 30 の分割はJSONの途中なので、この経路を通らない）。
    #[test]
    fn a_complete_json_without_its_newline_waits() {
        let mut d = SseDecoder::new();
        let (head, tail) = DELTA_A.split_at(DELTA_A.len() - 2);

        assert!(d.push(head).is_empty(), "改行が来る前に出している");
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

    // ---- OpenAI互換フレーバ -----------------------------------------------------------

    const OA_A: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n";
    const OA_B: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n";

    #[test]
    fn openai_deltas_come_out_in_order() {
        let mut d = SseDecoder::openai();
        assert_eq!(d.push(&format!("{OA_A}{OA_B}")), vec!["Hel".to_string(), "lo".to_string()]);
    }

    /// carry のロジックはフレーバ共通 — OpenAI形でも行の途中で切れたチャンクを落とさない。
    #[test]
    fn an_openai_event_split_across_chunks_is_not_lost() {
        let mut d = SseDecoder::openai();
        let (head, tail) = OA_A.split_at(20);

        assert!(d.push(head).is_empty(), "行が完成する前に何か返している");
        assert_eq!(d.push(tail), vec!["Hel".to_string()]);
    }

    /// 最後のチャンクは `content` が null で `finish_reason` だけを運ぶ。`role` だけの
    /// 最初のチャンクと、`choices` が空のチャンクも同様に何も出さない。
    #[test]
    fn openai_chunks_without_content_are_ignored() {
        let mut d = SseDecoder::openai();
        let noise = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":null},\"finish_reason\":\"stop\"}]}\n\n\
                     data: {\"choices\":[],\"usage\":{\"total_tokens\":9}}\n\n\
                     data: [DONE]\n\n";
        assert!(d.push(noise).is_empty());
    }

    /// フレーバは取り違えられない: 相手方の形は無視されるだけで、例外にも空振りの
    /// 文字列にもならない。
    #[test]
    fn each_flavor_ignores_the_others_events() {
        assert!(SseDecoder::openai().push(DELTA_A).is_empty());
        assert!(SseDecoder::new().push(OA_A).is_empty());
    }
}
