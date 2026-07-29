//! 初回起動時に生成するサンプル `Shougun.md`。

/// 初回生成用のサンプル。各セクションにコメント例を含み、そのままパース可能。
pub fn sample_markdown() -> String {
    // NOTE: 生のパスワード・API キーは書かないこと。
    r#"# Profile
- Role: あなたの職種を書いてください
- Industry: 業界
- Tools: Notion, Slack, GitHub

# Style
- Tone: 丁寧だがフレンドリー
- Length: まずは短く要点、その後に詳細

# Principles
- 判断に迷ったら最初に結論を出す

# DoNot
- 根拠のない数値は出さない
- 生のパスワードや API キーはこのファイルに書かない

# Workflows
- Name: DailyReview
  Trigger: "今日の振り返り"
  Steps:
    - 今日の出来事を 3 つに要約
    - 明日の最重要タスクを 1 つ決める

# Charm
- CoreStrengths:
  - あなたの強み（例：カオスから要点を抜き出せる）
- PersonaForOthers:
  - どう紹介されたいか
- PreferredIntroductionContexts:
  - 朝のレビューで今日の強みの活かしどころを一言ほしい
- NGCharmPatterns:
  - 過度に自己卑下するトーンは避けてほしい
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_config::parse::parse_shougun;

    #[test]
    fn sample_has_charm_and_warning_and_parses_clean() {
        let md = sample_markdown();
        assert!(md.contains("# Charm"));
        assert!(md.to_lowercase().contains("password") || md.contains("API"));
        let (_c, report) = parse_shougun(&md);
        assert!(report.ok, "sample must parse without errors: {:?}", report.section_errors);
    }
}
