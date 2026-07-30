//! 発話 + いまの画面 → プロンプト（Issue #44）。
//!
//! `ReplyContext` を直接受けない。あれは `db` feature の裏にいて、この変換自体はDBを必要と
//! しないから — 詰め替えは呼び出し側の仕事にして、ここはLinuxでもテストできる純関数に保つ。
//!
//! 添えるのは**すでに確度ゲートを通った事実**だけ。低確度の推測をここで事実として混ぜない
//! （CLAUDE.md: 低confidenceの状態を生成物に混ぜてはならない）。

/// プロンプトに添える、いまの状況。全て省略可能で、何も無ければ発話だけを投げる。
#[derive(Debug, Default, Clone)]
pub struct Spoken<'a> {
    /// 前面アプリの表示名。
    pub app: Option<&'a str>,
    /// 前面ウィンドウのタイトル。
    pub window_title: Option<&'a str>,
    /// 確度ゲート済みの事実。`ReplyContext::facts` がそのまま入る。
    pub facts: &'a [String],
}

/// プロンプトに載せる事実の上限。多いほど賢くなるわけではなく、初トークンまでの時間だけが
/// 確実に伸びる（SLO: 初トークン1s）。
const MAX_FACTS: usize = 12;

/// 発話と状況からプロンプトを組む。
///
/// 見出しは中身があるときだけ出す。空の "On screen:" が並ぶプロンプトは、モデルに
/// 「情報が無い」ではなく「情報を探せ」と読ませてしまう。
pub fn build_prompt(spoken: &str, ctx: &Spoken<'_>) -> String {
    let mut out = String::with_capacity(spoken.len() + 512);

    out.push_str(
        "You are SHOGUN, answering a question the user just spoke aloud while working. \
         Keep the answer brief and plain — it is shown in a small panel next to their work, \
         and may be read aloud. No preamble, no restating the question.\n\n",
    );

    match (ctx.app, ctx.window_title) {
        (Some(app), Some(title)) => {
            out.push_str(&format!("On screen: {app} — {title}\n"));
        }
        (Some(app), None) => out.push_str(&format!("On screen: {app}\n")),
        (None, Some(title)) => out.push_str(&format!("On screen: {title}\n")),
        (None, None) => {}
    }

    let facts: Vec<&String> = ctx.facts.iter().take(MAX_FACTS).collect();
    if !facts.is_empty() {
        out.push_str("Known about their work:\n");
        for f in facts {
            out.push_str("- ");
            out.push_str(f);
            out.push('\n');
        }
    }

    out.push_str("\nThey said: ");
    out.push_str(spoken.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spoken_words_are_always_present() {
        let out = build_prompt("summarise this page", &Spoken::default());
        assert!(out.contains("summarise this page"));
    }

    /// コンテキストが何も無くても成立する。キャッシュが冷えているのを待たない、という
    /// 設計判断がここに出る。
    #[test]
    fn no_context_still_produces_a_usable_prompt() {
        let out = build_prompt("what time is the standup", &Spoken::default());

        assert!(out.contains("what time is the standup"));
        assert!(!out.contains("On screen"), "空のコンテキスト見出しを出さない");
        assert!(!out.contains("Known"), "空の事実見出しを出さない");
    }

    #[test]
    fn the_foreground_window_is_included_when_known() {
        let ctx = Spoken { app: Some("Safari"), window_title: Some("Q3 plan"), ..Spoken::default() };
        let out = build_prompt("summarise this", &ctx);

        assert!(out.contains("Safari"));
        assert!(out.contains("Q3 plan"));
    }

    #[test]
    fn confidence_gated_facts_are_included() {
        let facts = vec!["You owe Aya a reply on the Q3 plan".to_string()];
        let ctx = Spoken { facts: &facts, ..Spoken::default() };
        let out = build_prompt("what do I owe", &ctx);

        assert!(out.contains("You owe Aya a reply on the Q3 plan"));
    }

    /// 事実が多すぎるとプロンプトが膨らんで初トークンが遅れる。上限で切る。
    #[test]
    fn the_fact_list_is_bounded() {
        let facts: Vec<String> = (0..50).map(|i| format!("fact number {i}")).collect();
        let ctx = Spoken { facts: &facts, ..Spoken::default() };
        let out = build_prompt("go", &ctx);

        assert!(out.contains("fact number 0"));
        assert!(!out.contains("fact number 20"), "上限を超えた事実が入っている");
    }

    /// 応答は読み上げられる可能性があり、パネルも小さい。短く答えるよう明示する。
    #[test]
    fn the_instruction_asks_for_a_short_spoken_style_answer() {
        let out = build_prompt("what is this", &Spoken::default());
        assert!(out.to_lowercase().contains("brief"));
    }
}
