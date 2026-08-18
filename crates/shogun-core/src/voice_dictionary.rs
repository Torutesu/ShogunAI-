//! Conservative local vocabulary matching for voice dictation.
//!
//! This layer corrects only explicit aliases. It deliberately does not use fuzzy, phonetic, or
//! edit-distance matching: a missed correction is safer than changing a name.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermScope {
    Global,
    Bundle(String),
    Surface(String),
}

#[derive(Debug, Clone)]
pub struct DictionaryEntry {
    pub canonical: String,
    pub aliases: Vec<String>,
    pub locale: Option<String>,
    pub scope: TermScope,
    pub priority: i32,
    pub enabled: bool,
    pub user_confirmed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DictionaryContext {
    pub locale: Option<String>,
    pub bundle_id: Option<String>,
    pub surface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedTerm {
    pub canonical: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryCorrection {
    pub text: String,
    pub protected_terms: Vec<String>,
    pub matches: Vec<MatchedTerm>,
}

#[derive(Debug, Clone, Default)]
pub struct VoiceDictionary {
    entries: Vec<DictionaryEntry>,
}

impl VoiceDictionary {
    pub fn new(entries: Vec<DictionaryEntry>) -> Self {
        Self { entries }
    }

    /// Canonical terms suitable for a bounded STT keyterm hint list.
    pub fn keyterms(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.canonical.clone())
            .collect()
    }

    /// Built-in aliases shipped with dictation cleanup.
    pub fn with_defaults() -> Self {
        let terms = [
            ("ShogunAI", &["shogun ai", "show gun ai"][..]),
            ("GPT-OSS", &["gpt oss", "g p t oss"][..]),
            ("Deepgram", &["deep gram"][..]),
            ("Nova-3", &["nova 3"][..]),
            ("Tauri", &["tarry"][..]),
            ("Rust", &["rust"][..]),
            ("Whisper", &["whisper"][..]),
            ("Groq", &["g rock"][..]),
        ];
        Self::new(
            terms
                .into_iter()
                .map(|(canonical, aliases)| DictionaryEntry {
                    canonical: canonical.to_string(),
                    aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
                    locale: None,
                    scope: TermScope::Global,
                    priority: 0,
                    enabled: true,
                    user_confirmed: true,
                })
                .collect(),
        )
    }

    pub fn correct(&self, input: &str, context: &DictionaryContext) -> DictionaryCorrection {
        let tokens = tokenize(input);
        let mut replacements = Vec::new();
        let mut index = 0;

        while index < tokens.len() {
            let candidates: Vec<(&DictionaryEntry, Vec<String>)> = self
                .entries
                .iter()
                .filter(|entry| {
                    entry.enabled && entry.user_confirmed && entry_matches_context(entry, context)
                })
                .filter_map(|entry| {
                    entry
                        .aliases
                        .iter()
                        .filter_map(|alias| {
                            let alias_tokens = normalized_tokens(alias);
                            (alias_tokens.len() > 0
                                && index + alias_tokens.len() <= tokens.len()
                                && tokens[index..index + alias_tokens.len()]
                                    .iter()
                                    .map(|token| token.normalized.as_str())
                                    .eq(alias_tokens.iter().map(String::as_str)))
                            .then_some((entry, alias_tokens))
                        })
                        .max_by_key(|(_, alias_tokens)| alias_tokens.len())
                })
                .collect();

            let Some((entry, alias_tokens)) = choose_candidate(candidates, context) else {
                index += 1;
                continue;
            };

            let start = tokens[index].start;
            let end = tokens[index + alias_tokens.len() - 1].end;
            replacements.push((start, end, entry.canonical.clone()));
            index += alias_tokens.len();
        }

        let mut output = String::with_capacity(input.len());
        let mut cursor = 0;
        let mut matches = Vec::with_capacity(replacements.len());
        for (start, end, canonical) in replacements {
            output.push_str(&input[cursor..start]);
            let output_start = output.len();
            output.push_str(&canonical);
            matches.push(MatchedTerm {
                canonical,
                start: output_start,
                end: output.len(),
            });
            cursor = end;
        }
        output.push_str(&input[cursor..]);

        let mut protected_terms = Vec::new();
        for matched in &matches {
            if !protected_terms
                .iter()
                .any(|term| term == &matched.canonical)
            {
                protected_terms.push(matched.canonical.clone());
            }
        }
        DictionaryCorrection {
            text: output,
            protected_terms,
            matches,
        }
    }
}

#[derive(Debug, Clone)]
struct Token {
    start: usize,
    end: usize,
    normalized: String,
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in input.char_indices() {
        if ch.is_alphanumeric() {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            tokens.push(Token {
                start: token_start,
                end: index,
                normalized: input[token_start..index].to_lowercase(),
            });
        }
    }
    if let Some(token_start) = start {
        tokens.push(Token {
            start: token_start,
            end: input.len(),
            normalized: input[token_start..].to_lowercase(),
        });
    }
    tokens
}

fn normalized_tokens(input: &str) -> Vec<String> {
    tokenize(input)
        .into_iter()
        .map(|token| token.normalized)
        .collect()
}

fn entry_matches_context(entry: &DictionaryEntry, context: &DictionaryContext) -> bool {
    if let Some(locale) = &entry.locale {
        if context.locale.as_deref() != Some(locale.as_str()) {
            return false;
        }
    }
    match &entry.scope {
        TermScope::Global => true,
        TermScope::Bundle(bundle) => context.bundle_id.as_deref() == Some(bundle.as_str()),
        TermScope::Surface(surface) => context.surface.as_deref() == Some(surface.as_str()),
    }
}

fn choose_candidate<'a>(
    candidates: Vec<(&'a DictionaryEntry, Vec<String>)>,
    context: &DictionaryContext,
) -> Option<(&'a DictionaryEntry, Vec<String>)> {
    let max_len = candidates.iter().map(|(_, alias)| alias.len()).max()?;
    let mut candidates = candidates
        .into_iter()
        .filter(|(_, alias)| alias.len() == max_len)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(entry, _)| {
        let scope_rank = match &entry.scope {
            TermScope::Global => 0,
            TermScope::Surface(surface) if context.surface.as_deref() == Some(surface) => 2,
            TermScope::Bundle(bundle) if context.bundle_id.as_deref() == Some(bundle) => 3,
            _ => 1,
        };
        (scope_rank, entry.priority)
    });
    let best = candidates.pop()?;
    let tied = candidates.iter().any(|(entry, alias)| {
        alias.len() == best.1.len()
            && entry.priority == best.0.priority
            && std::mem::discriminant(&entry.scope) == std::mem::discriminant(&best.0.scope)
    });
    (!tied).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(canonical: &str, aliases: &[&str]) -> DictionaryEntry {
        DictionaryEntry {
            canonical: canonical.into(),
            aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
            locale: None,
            scope: TermScope::Global,
            priority: 0,
            enabled: true,
            user_confirmed: true,
        }
    }

    #[test]
    fn corrects_unique_alias_and_returns_protected_term() {
        let result = VoiceDictionary::new(vec![entry("ShogunAI", &["shogun ai"])])
            .correct("build shogun ai today", &DictionaryContext::default());
        assert_eq!(result.text, "build ShogunAI today");
        assert_eq!(result.protected_terms, vec!["ShogunAI"]);
    }

    #[test]
    fn defaults_preserve_groq_alias_behavior() {
        let result =
            VoiceDictionary::with_defaults().correct("use g rock", &DictionaryContext::default());
        assert_eq!(result.text, "use Groq");
    }

    #[test]
    fn prefers_longest_alias() {
        let result = VoiceDictionary::new(vec![entry("GPT-OSS", &["gpt", "gpt oss"])])
            .correct("gpt oss model", &DictionaryContext::default());
        assert_eq!(result.text, "GPT-OSS model");
    }

    #[test]
    fn skips_ambiguous_aliases() {
        let first = entry("Alpha", &["alpha"]);
        let second = entry("Beta", &["alpha"]);
        let result = VoiceDictionary::new(vec![first, second])
            .correct("alpha", &DictionaryContext::default());
        assert_eq!(result.text, "alpha");
    }

    #[test]
    fn does_not_replace_inside_a_word() {
        let result = VoiceDictionary::new(vec![entry("Rust", &["rust"])])
            .correct("trust rust", &DictionaryContext::default());
        assert_eq!(result.text, "trust Rust");
    }

    #[test]
    fn prefers_bundle_scope_over_global_scope() {
        let mut global = entry("global", &["draft"]);
        let mut scoped = entry("scoped", &["draft"]);
        scoped.scope = TermScope::Bundle("com.example.app".into());
        global.priority = 100;
        let result = VoiceDictionary::new(vec![global, scoped]).correct(
            "draft",
            &DictionaryContext {
                bundle_id: Some("com.example.app".into()),
                ..Default::default()
            },
        );
        assert_eq!(result.text, "scoped");
    }
}
