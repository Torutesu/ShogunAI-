//! `ShougunConfig` を system prompt 用の "User Directives" 文字列に変換する。

use crate::user_config::model::ShougunConfig;

fn push_list(out: &mut String, label: &str, items: &[String]) {
    let items: Vec<&String> = items.iter().filter(|s| !s.trim().is_empty()).collect();
    if items.is_empty() {
        return;
    }
    out.push_str(label);
    out.push('\n');
    for it in items {
        out.push_str("- ");
        out.push_str(it.trim());
        out.push('\n');
    }
    out.push('\n');
}

/// user-facing 生成の system prompt 先頭に差し込むブロックを生成する。
/// 何も設定が無ければ空文字列を返す（呼び出し側はそのまま連結できる）。
pub fn render_directives(cfg: &ShougunConfig) -> String {
    let mut out = String::new();

    // NOTE: profile.* (role/industry/tools/topics) is parsed but intentionally NOT rendered
    // here — like preferred_intro_contexts, it is reserved for Morning Brief / discovery
    // (後続タスク), not the standing system-prompt directive block.

    if !cfg.style.tone.trim().is_empty() {
        out.push_str(&format!("Tone: {}\n", cfg.style.tone.trim()));
    }
    if !cfg.style.length.trim().is_empty() {
        out.push_str(&format!("Length preference: {}\n", cfg.style.length.trim()));
    }
    push_list(&mut out, "Format preferences:", &cfg.style.format_hints);
    push_list(&mut out, "Principles to follow:", &cfg.principles);
    push_list(&mut out, "Never do the following:", &cfg.do_not);

    // NOTE: charm.preferred_intro_contexts は「いつ・どう自己紹介を出すか」のタイミング
    // 設定であり、常時の system-prompt directive ではない。Morning Brief / 紹介文生成
    // （後続タスク）で参照するため、ここでは意図的に出力しない。
    if !cfg.charm_disabled {
        push_list(&mut out, "The user's strengths (draw on these):", &cfg.charm.core_strengths);
        push_list(&mut out, "How to present the user to others:", &cfg.charm.persona_for_others);
        push_list(&mut out, "Avoid these ways of framing the user:", &cfg.charm.ng_charm_patterns);
    }

    for w in &cfg.workflows {
        if w.trigger.trim().is_empty() || w.steps.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "When the user says \"{}\", follow these steps: {}\n",
            w.trigger.trim(),
            w.steps.join(" → ")
        ));
    }

    if out.trim().is_empty() {
        return String::new();
    }
    format!("User Directives (the user has declared these preferences — honor them):\n{out}")
}

/// L5 の自動学習 lesson を directive ブロックへ渡すビュー（Plan D-5a）。shogun-memory には
/// 依存しない（user_config は純粋層）ので、daemon が `lessons::Lesson` からこの形へ写す。
/// instruction と confidence のみ — feedback 本文はこの型に存在しない。
#[derive(Debug, Clone, PartialEq)]
pub struct LearnedLesson {
    /// プロンプト注入可能な1文（英語、蒸留テンプレ由来）。
    pub instruction: String,
    /// 0.0..=1.0。Low帯（<0.5、fusion の band gate と同じ閾値）はここでも除外する。
    pub confidence: f64,
}

/// Low/Medium 境界。`shogun_fusion::confidence::band()` の 0.5 と同じ値（依存方向の都合で
/// 値の複製。shogun-memory の `INJECTION_FLOOR` も同じ 0.5）。
const LEARNED_CONFIDENCE_FLOOR: f64 = 0.5;

/// `## Learned (auto)` セクションを描画する（Plan D-5a）。呼び出し側（daemon）が
/// `active_lessons` で scope フィルタ＋confidence 降順 top-k を済ませた列を渡す想定。
/// ここでも Low 帯と空 instruction を防御的に落とす。**active な lesson が無ければ空文字列**
/// — セクション見出しごと出さない。
pub fn render_learned_section(lessons: &[LearnedLesson]) -> String {
    let items: Vec<&LearnedLesson> = lessons
        .iter()
        .filter(|l| l.confidence >= LEARNED_CONFIDENCE_FLOOR && !l.instruction.trim().is_empty())
        .collect();
    if items.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Learned (auto)\n\
         Auto-learned from the user's past corrections (not user-authored). These adjust\n\
         generated content only — they never change which actions need confirmation:\n",
    );
    for l in items {
        out.push_str("- ");
        out.push_str(l.instruction.trim());
        out.push('\n');
    }
    out
}

/// [`render_directives`] ＋ `## Learned (auto)` セクション（Plan D-5a）。手書き directive と
/// 自動学習分は見出しで明確に分離される。両方空なら空文字列（呼び出し側はそのまま連結可）。
pub fn render_directives_with_lessons(cfg: &ShougunConfig, lessons: &[LearnedLesson]) -> String {
    let base = render_directives(cfg);
    let learned = render_learned_section(lessons);
    match (base.is_empty(), learned.is_empty()) {
        (true, true) => String::new(),
        (false, true) => base,
        (true, false) => learned,
        (false, false) => format!("{base}\n{learned}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_config::parse::parse_shougun;

    #[test]
    fn empty_config_yields_empty_string() {
        assert_eq!(render_directives(&ShougunConfig::default()), "");
    }

    #[test]
    fn includes_principles_and_donot_and_charm() {
        let (c, _) = parse_shougun(
            "# Principles\n- データドリブン\n# DoNot\n- 数値を捏造しない\n# Charm\n- CoreStrengths:\n  - 比喩がうまい\n",
        );
        let out = render_directives(&c);
        assert!(out.contains("User Directives"));
        assert!(out.contains("データドリブン"));
        assert!(out.contains("数値を捏造しない"));
        assert!(out.contains("比喩がうまい"));
    }

    #[test]
    fn disabled_charm_is_omitted() {
        let mut c = ShougunConfig::default();
        c.charm.core_strengths = vec!["x".into()];
        c.charm_disabled = true;
        assert!(!render_directives(&c).contains('x'));
    }

    // ---------------------------------------------------------- Learned (auto) — Plan D-5a

    fn learned(instruction: &str, confidence: f64) -> LearnedLesson {
        LearnedLesson { instruction: instruction.into(), confidence }
    }

    #[test]
    fn learned_section_lists_instructions_under_its_own_heading() {
        let (cfg, _) = parse_shougun("# Principles\n- データドリブン\n");
        let lessons =
            [learned("Write replies in English.", 0.7), learned("Keep drafts shorter.", 0.55)];
        let out = render_directives_with_lessons(&cfg, &lessons);
        // user-authored part first, clearly separated from the auto section
        assert!(out.contains("User Directives"));
        assert!(out.contains("データドリブン"));
        let heading = out.find("## Learned (auto)").expect("learned heading present");
        assert!(out.find("データドリブン").unwrap() < heading, "user-authored comes first");
        assert!(out.contains("- Write replies in English.\n"));
        assert!(out.contains("- Keep drafts shorter.\n"));
        assert!(out.contains("never change which actions need confirmation"));
    }

    #[test]
    fn learned_section_is_omitted_entirely_when_no_active_lessons() {
        let (cfg, _) = parse_shougun("# Principles\n- データドリブン\n");
        // empty, low-band only, and blank-instruction lists all omit the section wholesale
        for lessons in [vec![], vec![learned("shaky", 0.49)], vec![learned("  ", 0.9)]] {
            let out = render_directives_with_lessons(&cfg, &lessons);
            assert!(!out.contains("Learned"), "no heading without injectable lessons: {out}");
            assert_eq!(out, render_directives(&cfg), "base directives unchanged");
        }
        // and with an empty config too, the whole render stays empty
        assert_eq!(render_directives_with_lessons(&ShougunConfig::default(), &[]), "");
    }

    #[test]
    fn learned_only_config_renders_just_the_learned_section() {
        let out = render_directives_with_lessons(
            &ShougunConfig::default(),
            &[learned("Write replies in English.", 0.7)],
        );
        assert!(out.starts_with("## Learned (auto)"));
        assert!(!out.contains("User Directives"));
    }

    #[test]
    fn learned_section_drops_low_band_lessons() {
        let out = render_learned_section(&[
            learned("solid", 0.5),
            learned("shaky guess", 0.49), // below the fusion Low/Medium boundary
        ]);
        assert!(out.contains("- solid\n"));
        assert!(!out.contains("shaky"), "Low band must never be injected: {out}");
    }
}
