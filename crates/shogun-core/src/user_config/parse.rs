//! `Shougun.md` の行ベースパーサ（fail-soft）。

use crate::user_config::model::*;

/// 見出し `# X` で本文を分割する。戻り値は (heading, start_line, body_lines)。
/// `#` 1個の ATX 見出しのみをセクション境界とする（`##` 以下は本文扱い）。
pub(crate) fn split_sections(input: &str) -> Vec<(String, usize, Vec<String>)> {
    let mut out: Vec<(String, usize, Vec<String>)> = Vec::new();
    let mut cur: Option<(String, usize, Vec<String>)> = None;
    for (i, raw) in input.lines().enumerate() {
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some(sec) = cur.take() {
                out.push(sec);
            }
            cur = Some((rest.trim().to_string(), i + 1, Vec::new()));
        } else if let Some((_, _, body)) = cur.as_mut() {
            body.push(line.to_string());
        }
    }
    if let Some(sec) = cur.take() {
        out.push(sec);
    }
    out
}

/// bullet 行 `- text` を取り出す（トップレベルの箇条書き用。インデントは無視する）。
fn bullets(body: &[String]) -> Vec<String> {
    body.iter()
        .filter_map(|l| l.trim().strip_prefix("- ").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// `- Key: value` 形式から key に一致する値を返す（大文字小文字無視）。
fn field(body: &[String], key: &str) -> String {
    for l in body {
        let t = l.trim().trim_start_matches("- ").trim();
        if let Some((k, v)) = t.split_once(':') {
            if k.trim().eq_ignore_ascii_case(key) {
                return v.trim().trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

fn csv(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// `- Key:` の子要素を集める。子は「キー行より深いインデントの bullet」で判定する
/// （末尾がコロンの項目でも打ち切らない）。`- Key: value` の inline 値も 1 項目として拾う。
fn sub_bullets(body: &[String], key: &str) -> Vec<String> {
    let key_lc = key.to_ascii_lowercase();
    let key_prefix = format!("{key_lc}:");
    let mut out = Vec::new();
    let mut key_indent: Option<usize> = None;
    for l in body {
        let indent = l.len() - l.trim_start().len();
        let t = l.trim();
        if !t.starts_with("- ") {
            continue;
        }
        let content = t.trim_start_matches("- ").trim();
        match key_indent {
            None => {
                if content.to_ascii_lowercase().starts_with(&key_prefix) {
                    key_indent = Some(indent);
                    if let Some((_, v)) = content.split_once(':') {
                        let v = v.trim();
                        if !v.is_empty() {
                            out.push(v.to_string());
                        }
                    }
                }
            }
            Some(ki) => {
                if indent > ki {
                    if !content.is_empty() {
                        out.push(content.to_string());
                    }
                } else {
                    break;
                }
            }
        }
    }
    out
}

fn parse_workflows(body: &[String]) -> Vec<Workflow> {
    let mut out = Vec::new();
    let mut cur: Option<Workflow> = None;
    let mut in_steps = false;
    for l in body {
        let t = l.trim();
        let keyed = t.trim_start_matches("- ").trim();
        if let Some(name) = keyed.strip_prefix("Name:") {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            cur = Some(Workflow { name: name.trim().to_string(), ..Default::default() });
            in_steps = false;
        } else if let Some(tr) = keyed.strip_prefix("Trigger:") {
            if let Some(w) = cur.as_mut() {
                w.trigger = tr.trim().trim_matches('"').to_string();
            }
        } else if keyed.eq_ignore_ascii_case("Steps:") {
            in_steps = true;
        } else if in_steps {
            if let Some(step) = t.strip_prefix("- ") {
                if let Some(w) = cur.as_mut() {
                    w.steps.push(step.trim().to_string());
                }
            }
        }
    }
    if let Some(w) = cur.take() {
        out.push(w);
    }
    out
}

/// `Shougun.md` の内容をパースする。セクション単位でエラーを隔離する。
pub fn parse_shougun(input: &str) -> (ShougunConfig, ParseReport) {
    let mut cfg = ShougunConfig::default();
    let mut report = ParseReport { ok: true, section_errors: Vec::new() };

    for (heading, line, body) in split_sections(input) {
        match heading.as_str() {
            "Profile" => {
                cfg.profile.role = field(&body, "Role");
                cfg.profile.industry = field(&body, "Industry");
                cfg.profile.tools = csv(&field(&body, "Tools"));
                cfg.profile.topics = csv(&field(&body, "Topics"));
            }
            "Style" => {
                cfg.style.tone = field(&body, "Tone");
                cfg.style.length = field(&body, "Length");
                cfg.style.format_hints = sub_bullets(&body, "Format");
            }
            "Principles" => cfg.principles = bullets(&body),
            "DoNot" => cfg.do_not = bullets(&body),
            "Workflows" => cfg.workflows = parse_workflows(&body),
            "Charm" => {
                cfg.charm.core_strengths = sub_bullets(&body, "CoreStrengths");
                cfg.charm.persona_for_others = sub_bullets(&body, "PersonaForOthers");
                cfg.charm.preferred_intro_contexts =
                    sub_bullets(&body, "PreferredIntroductionContexts");
                cfg.charm.ng_charm_patterns = sub_bullets(&body, "NGCharmPatterns");
                // 4項目すべて空 かつ 本文はあり → フォーマット不正として Charm 無効化
                let all_empty = cfg.charm.core_strengths.is_empty()
                    && cfg.charm.persona_for_others.is_empty()
                    && cfg.charm.preferred_intro_contexts.is_empty()
                    && cfg.charm.ng_charm_patterns.is_empty();
                if all_empty && body.iter().any(|l| !l.trim().is_empty()) {
                    cfg.charm_disabled = true;
                    report.ok = false;
                    report.section_errors.push(SectionError {
                        section: "Charm".into(),
                        line,
                        message: "Charm セクションから認識可能な項目を抽出できませんでした".into(),
                    });
                }
            }
            other => cfg.unknown_sections.push(RawSection {
                heading: other.to_string(),
                body: body.join("\n"),
            }),
        }
    }
    (cfg, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_h1_headings_only() {
        let md = "# Profile\n- Role: PM\n\n# Style\n- Tone: warm\n## Sub\ntext";
        let secs = split_sections(md);
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].0, "Profile");
        assert_eq!(secs[0].1, 1); // line number (1-based)
        assert_eq!(secs[1].0, "Style");
        // `## Sub` は Style の本文として残る
        assert!(secs[1].2.iter().any(|l| l == "## Sub"));
    }

    #[test]
    fn text_before_first_heading_is_ignored() {
        let secs = split_sections("intro text\n# Profile\n- Role: PM");
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].0, "Profile");
    }

    const EXAMPLE: &str = r#"# Profile
- Role: B2B SaaS のプロダクトマネージャー
- Industry: ホテル向けレベニューマネジメント
- Tools: Notion, Slack, GitHub

# Style
- Tone: 丁寧だがフレンドリー
- Length: まずは短く要点、その後に詳細

# Principles
- データドリブンであることを優先する
- ユーザー価値 > 売上 > コスト の順で判断する

# DoNot
- 根拠のない数値は出さない

# Workflows
- Name: DailyReview
  Trigger: "今日の振り返り"
  Steps:
    - 今日の出来事を 3 つに要約
    - 明日の最重要タスクを 1 つだけ決める

# Charm
- CoreStrengths:
  - 抽象度の高い概念を比喩に落とし込める
- NGCharmPatterns:
  - 過度に自己卑下するトーンは避けてほしい
"#;

    #[test]
    fn parses_example_sections() {
        let (c, report) = parse_shougun(EXAMPLE);
        assert!(report.ok, "errors: {:?}", report.section_errors);
        assert_eq!(c.profile.role, "B2B SaaS のプロダクトマネージャー");
        assert_eq!(c.profile.tools, vec!["Notion", "Slack", "GitHub"]);
        assert_eq!(c.style.tone, "丁寧だがフレンドリー");
        assert_eq!(c.principles.len(), 2);
        assert_eq!(c.do_not, vec!["根拠のない数値は出さない"]);
        assert_eq!(c.workflows.len(), 1);
        assert_eq!(c.workflows[0].name, "DailyReview");
        assert_eq!(c.workflows[0].trigger, "今日の振り返り");
        assert_eq!(c.workflows[0].steps.len(), 2);
        assert_eq!(c.charm.core_strengths.len(), 1);
        assert_eq!(c.charm.ng_charm_patterns.len(), 1);
        assert!(!c.charm_disabled);
    }

    #[test]
    fn unknown_heading_is_preserved() {
        let (c, _) = parse_shougun("# Notes\n- hello\n");
        assert_eq!(c.unknown_sections.len(), 1);
        assert_eq!(c.unknown_sections[0].heading, "Notes");
    }

    #[test]
    fn empty_input_is_ok() {
        let (c, report) = parse_shougun("");
        assert!(report.ok);
        assert_eq!(c, ShougunConfig::default());
    }

    #[test]
    fn sub_bullets_keeps_items_after_a_colon_ending_item() {
        let (c, _) = parse_shougun(
            "# Charm\n- CoreStrengths:\n  - 強みその1\n  - 末尾がコロン:\n  - 強みその3\n",
        );
        assert_eq!(c.charm.core_strengths, vec!["強みその1", "末尾がコロン:", "強みその3"]);
    }

    #[test]
    fn sub_bullets_captures_inline_value() {
        let (c, _) = parse_shougun("# Charm\n- CoreStrengths: 比喩がうまい\n");
        assert_eq!(c.charm.core_strengths, vec!["比喩がうまい"]);
        assert!(!c.charm_disabled);
    }

    #[test]
    fn multiple_charm_subkeys_are_separated() {
        let (c, _) = parse_shougun(
            "# Charm\n- CoreStrengths:\n  - A\n- NGCharmPatterns:\n  - B\n",
        );
        assert_eq!(c.charm.core_strengths, vec!["A"]);
        assert_eq!(c.charm.ng_charm_patterns, vec!["B"]);
    }
}
