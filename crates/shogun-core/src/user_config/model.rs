//! Shougun.md の内部データモデル（DB/I-O 非依存）。

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub role: String,
    pub industry: String,
    pub tools: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Style {
    pub tone: String,
    pub length: String,
    pub format_hints: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Workflow {
    pub name: String,
    pub trigger: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Charm {
    pub core_strengths: Vec<String>,
    pub persona_for_others: Vec<String>,
    pub preferred_intro_contexts: Vec<String>,
    pub ng_charm_patterns: Vec<String>,
}

/// 未知見出しは破棄せず保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RawSection {
    pub heading: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ShougunConfig {
    pub profile: Profile,
    pub style: Style,
    pub principles: Vec<String>,
    pub do_not: Vec<String>,
    pub workflows: Vec<Workflow>,
    pub charm: Charm,
    /// `# Charm` のパースに失敗したら true（Charm 機能のみ無効化する）。
    pub charm_disabled: bool,
    pub unknown_sections: Vec<RawSection>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SectionError {
    pub section: String,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParseReport {
    pub ok: bool,
    pub section_errors: Vec<SectionError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_empty() {
        let c = ShougunConfig::default();
        assert!(c.principles.is_empty());
        assert!(!c.charm_disabled);
        assert!(c.unknown_sections.is_empty());
    }
}
