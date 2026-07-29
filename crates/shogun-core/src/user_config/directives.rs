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
}
