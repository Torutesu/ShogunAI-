//! Local, user-confirmed vocabulary for speech recognition.
//!
//! Terms and aliases never leave the encrypted memory database on their own. Callers select a
//! small, context-eligible hint list before any ASR request; local matching remains the authority
//! for deterministic corrections.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Hard local bounds keep dictation-time matching predictable. Explicit aliases are intentionally
/// small vocabulary, not a general phrase-expansion store.
pub const MAX_VOICE_TERMS: usize = 500;
pub const MAX_ALIASES_PER_TERM: usize = 16;
pub const MAX_VOICE_ALIASES: usize = 5_000;
pub const MAX_SCOPE_REF_CHARS: usize = 255;

/// Where a voice term is eligible to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceTermScope {
    Global,
    Bundle,
    Surface,
}

impl VoiceTermScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Bundle => "bundle",
            Self::Surface => "surface",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "global" => Self::Global,
            "bundle" => Self::Bundle,
            "surface" => Self::Surface,
            _ => return None,
        })
    }
}

/// Origin of a persisted term. Automatic learning is deliberately not a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceTermProvenance {
    User,
}

impl VoiceTermProvenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "user" => Self::User,
            _ => return None,
        })
    }
}

/// One user-managed canonical term and its explicit aliases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceTerm {
    pub id: i64,
    pub canonical: String,
    pub aliases: Vec<String>,
    pub locale: Option<String>,
    pub scope: VoiceTermScope,
    pub scope_ref: Option<String>,
    pub priority: i32,
    pub enabled: bool,
    pub provenance: VoiceTermProvenance,
}

/// Validated input for creating or replacing a user-managed voice term.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NewVoiceTerm {
    pub canonical: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub scope: Option<VoiceTermScope>,
    #[serde(default)]
    pub scope_ref: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

fn invalid_input(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}

fn normalized_alias(value: &str) -> String {
    let mut tokens = Vec::new();
    let mut token = String::new();
    for ch in value.chars() {
        if is_lookup_token_char(ch) {
            token.push(ch);
        } else if !token.is_empty() {
            tokens.push(normalize_token(&token));
            token.clear();
        }
    }
    if !token.is_empty() {
        tokens.push(normalize_token(&token));
    }
    tokens.join(" ")
}

fn is_lookup_token_char(ch: char) -> bool {
    ch.is_alphanumeric()
        || matches!(
            ch as u32,
            0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
        )
}

fn normalize_token(value: &str) -> String {
    value.nfkc().flat_map(char::to_lowercase).collect()
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Persist only the locale shape used by dictionary matching: language or language-region.
/// Rejecting unsupported values keeps a malformed stored locale from later normalizing to `None`
/// and unexpectedly behaving like an unscoped locale rule.
fn normalized_locale(value: Option<&str>) -> Result<Option<String>, rusqlite::Error> {
    let Some(value) = normalized_optional(value) else {
        return Ok(None);
    };
    let value = value.replace('_', "-");
    let parts = value.split('-').collect::<Vec<_>>();
    let valid_language = parts.first().is_some_and(|language| {
        (2..=3).contains(&language.len()) && language.chars().all(|ch| ch.is_ascii_alphabetic())
    });
    let valid_region = match parts.as_slice() {
        [_] => true,
        [_, region] => {
            (region.len() == 2 && region.chars().all(|ch| ch.is_ascii_alphabetic()))
                || (region.len() == 3 && region.chars().all(|ch| ch.is_ascii_digit()))
        }
        _ => false,
    };
    if !valid_language || !valid_region {
        return Err(invalid_input(
            "locale must be a language or language-region tag, for example en or en-US",
        ));
    }
    let language = parts[0].to_ascii_lowercase();
    let normalized = match parts.as_slice() {
        [_] => language,
        [_, region] if region.chars().all(|ch| ch.is_ascii_alphabetic()) => {
            format!("{language}-{}", region.to_ascii_uppercase())
        }
        [_, region] => format!("{language}-{region}"),
        _ => unreachable!("validated locale has at most two parts"),
    };
    Ok(Some(normalized))
}

type ValidatedVoiceTerm = (
    String,
    Vec<(String, String)>,
    Option<String>,
    VoiceTermScope,
    Option<String>,
    i32,
);

fn validate(input: &NewVoiceTerm) -> Result<ValidatedVoiceTerm, rusqlite::Error> {
    let canonical = input.canonical.trim();
    if canonical.is_empty()
        || canonical.chars().count() > 120
        || canonical.chars().any(char::is_control)
    {
        return Err(invalid_input(
            "canonical must be 1-120 non-control characters",
        ));
    }
    let canonical = canonical.to_string();
    let scope = input.scope.unwrap_or(VoiceTermScope::Global);
    let scope_ref = normalized_optional(input.scope_ref.as_deref());
    if scope_ref.as_ref().is_some_and(|value| {
        value.chars().count() > MAX_SCOPE_REF_CHARS || value.chars().any(char::is_control)
    }) {
        return Err(invalid_input(
            "scope_ref must be 1-255 non-control characters",
        ));
    }
    match scope {
        VoiceTermScope::Global if scope_ref.is_some() => {
            return Err(invalid_input("global terms cannot have scope_ref"));
        }
        VoiceTermScope::Bundle | VoiceTermScope::Surface if scope_ref.is_none() => {
            return Err(invalid_input("bundle and surface terms require scope_ref"));
        }
        VoiceTermScope::Bundle | VoiceTermScope::Surface => {}
        VoiceTermScope::Global => {}
    }

    let mut aliases = Vec::with_capacity(input.aliases.len() + 1);
    for alias in input
        .aliases
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(canonical.as_str()))
    {
        let alias = alias.trim();
        if alias.is_empty() || alias.chars().count() > 120 || alias.chars().any(char::is_control) {
            return Err(invalid_input(
                "aliases must be 1-120 non-control characters",
            ));
        }
        let normalized = normalized_alias(alias);
        if normalized.is_empty() {
            return Err(invalid_input(
                "alias must contain at least one letter or number",
            ));
        }
        if !aliases.iter().any(|(_, existing)| existing == &normalized) {
            aliases.push((alias.to_string(), normalized));
        }
    }
    if aliases.len() > MAX_ALIASES_PER_TERM {
        return Err(invalid_input(
            "a voice term may have at most 16 aliases including its canonical spelling",
        ));
    }
    Ok((
        canonical,
        aliases,
        normalized_locale(input.locale.as_deref())?,
        scope,
        scope_ref,
        input.priority.unwrap_or_default(),
    ))
}

fn parse_error(field: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        format!("unknown voice term {field}: {value:?}").into(),
    )
}

fn aliases_for(conn: &Connection, term_id: i64) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement =
        conn.prepare("SELECT alias FROM voice_term_aliases WHERE term_id = ?1 ORDER BY id ASC")?;
    let rows = statement.query_map([term_id], |row| row.get(0))?;
    rows.collect()
}

fn term_from_row(conn: &Connection, row: &rusqlite::Row<'_>) -> Result<VoiceTerm, rusqlite::Error> {
    let id: i64 = row.get(0)?;
    let scope: String = row.get(3)?;
    let provenance: String = row.get(7)?;
    Ok(VoiceTerm {
        id,
        canonical: row.get(1)?,
        aliases: aliases_for(conn, id)?,
        locale: row.get(2)?,
        scope: VoiceTermScope::parse(&scope).ok_or_else(|| parse_error("scope", &scope))?,
        scope_ref: row.get(4)?,
        priority: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        provenance: VoiceTermProvenance::parse(&provenance)
            .ok_or_else(|| parse_error("provenance", &provenance))?,
    })
}

/// List all user terms, disabled included, for settings and local ASR assembly.
pub fn list_voice_terms(conn: &Connection) -> Result<Vec<VoiceTerm>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, canonical, locale, scope, scope_ref, priority, enabled, provenance
         FROM voice_terms ORDER BY enabled DESC, priority DESC, id ASC",
    )?;
    let mut rows = statement.query([])?;
    let mut terms = Vec::new();
    while let Some(row) = rows.next()? {
        terms.push(term_from_row(conn, row)?);
    }
    Ok(terms)
}

fn get_voice_term(conn: &Connection, id: i64) -> Result<Option<VoiceTerm>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, canonical, locale, scope, scope_ref, priority, enabled, provenance
         FROM voice_terms WHERE id = ?1",
    )?;
    let mut rows = statement.query([id])?;
    match rows.next()? {
        Some(row) => term_from_row(conn, row).map(Some),
        None => Ok(None),
    }
}

fn replace_aliases(
    tx: &rusqlite::Transaction<'_>,
    term_id: i64,
    aliases: &[(String, String)],
) -> Result<(), rusqlite::Error> {
    tx.execute(
        "DELETE FROM voice_term_aliases WHERE term_id = ?1",
        [term_id],
    )?;
    let mut statement = tx.prepare(
        "INSERT INTO voice_term_aliases (term_id, alias, alias_normalized) VALUES (?1, ?2, ?3)",
    )?;
    for (alias, normalized) in aliases {
        statement.execute(params![term_id, alias, normalized])?;
    }
    Ok(())
}

/// Create one explicit user-confirmed term. Its canonical spelling is also an exact local alias.
pub fn create_voice_term(
    conn: &mut Connection,
    input: &NewVoiceTerm,
    now_ms: i64,
) -> Result<VoiceTerm, rusqlite::Error> {
    let (canonical, aliases, locale, scope, scope_ref, priority) = validate(input)?;
    let tx = conn.transaction()?;
    let term_count: usize =
        tx.query_row("SELECT count(*) FROM voice_terms", [], |row| row.get(0))?;
    let alias_count: usize =
        tx.query_row("SELECT count(*) FROM voice_term_aliases", [], |row| {
            row.get(0)
        })?;
    if term_count >= MAX_VOICE_TERMS || alias_count + aliases.len() > MAX_VOICE_ALIASES {
        return Err(invalid_input("voice dictionary capacity reached"));
    }
    tx.execute(
        "INSERT INTO voice_terms
           (canonical, locale, scope, scope_ref, priority, enabled, provenance, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![canonical, locale, scope.as_str(), scope_ref, priority, input.enabled as i64, VoiceTermProvenance::User.as_str(), now_ms],
    )?;
    let id = tx.last_insert_rowid();
    replace_aliases(&tx, id, &aliases)?;
    tx.commit()?;
    get_voice_term(conn, id)?.ok_or_else(|| invalid_input("created voice term disappeared"))
}

/// Replace one term and its aliases atomically. `None` means the id does not exist.
pub fn update_voice_term(
    conn: &mut Connection,
    id: i64,
    input: &NewVoiceTerm,
    now_ms: i64,
) -> Result<Option<VoiceTerm>, rusqlite::Error> {
    let (canonical, aliases, locale, scope, scope_ref, priority) = validate(input)?;
    let tx = conn.transaction()?;
    let old_alias_count: usize = tx.query_row(
        "SELECT count(*) FROM voice_term_aliases WHERE term_id = ?1",
        [id],
        |row| row.get(0),
    )?;
    let alias_count: usize =
        tx.query_row("SELECT count(*) FROM voice_term_aliases", [], |row| {
            row.get(0)
        })?;
    if alias_count.saturating_sub(old_alias_count) + aliases.len() > MAX_VOICE_ALIASES {
        return Err(invalid_input("voice dictionary capacity reached"));
    }
    let changed = tx.execute(
        "UPDATE voice_terms
         SET canonical = ?1, locale = ?2, scope = ?3, scope_ref = ?4, priority = ?5,
             enabled = ?6, updated_at_ms = ?7
         WHERE id = ?8",
        params![
            canonical,
            locale,
            scope.as_str(),
            scope_ref,
            priority,
            input.enabled as i64,
            now_ms,
            id
        ],
    )?;
    if changed == 0 {
        tx.rollback()?;
        return Ok(None);
    }
    replace_aliases(&tx, id, &aliases)?;
    tx.commit()?;
    get_voice_term(conn, id)
}

/// Delete a term and its aliases. Returns false when the id is absent.
pub fn delete_voice_term(conn: &Connection, id: i64) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute("DELETE FROM voice_terms WHERE id = ?1", [id])? != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(canonical: &str, aliases: &[&str]) -> NewVoiceTerm {
        NewVoiceTerm {
            canonical: canonical.into(),
            aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
            locale: None,
            scope: None,
            scope_ref: None,
            priority: None,
            enabled: true,
        }
    }

    #[test]
    fn storage_round_trip_keeps_canonical_and_explicit_aliases() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let stored = create_voice_term(&mut conn, &input("ShogunAI", &["show gun ai"]), 10)
            .expect("store term");
        assert_eq!(stored.aliases, vec!["show gun ai", "ShogunAI"]);
        assert_eq!(list_voice_terms(&conn).expect("list terms"), vec![stored]);
    }

    #[test]
    fn update_replaces_aliases_without_leaving_old_matches() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let created = create_voice_term(&mut conn, &input("ShogunAI", &["show gun ai"]), 10)
            .expect("store term");
        let updated =
            update_voice_term(&mut conn, created.id, &input("Shogun AI", &["shogun"]), 20)
                .expect("update term")
                .expect("term exists");
        assert_eq!(updated.aliases, vec!["shogun", "Shogun AI"]);
        assert_eq!(updated.canonical, "Shogun AI");
    }

    #[test]
    fn scoped_terms_require_a_scope_reference() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let mut term = input("Tauri", &["tarry"]);
        term.scope = Some(VoiceTermScope::Bundle);
        let error = create_voice_term(&mut conn, &term, 10).expect_err("scope ref required");
        assert!(error.to_string().contains("scope_ref"));
    }

    #[test]
    fn scoped_terms_reject_oversized_or_control_scope_references() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let mut term = input("Tauri", &["tarry"]);
        term.scope = Some(VoiceTermScope::Bundle);
        term.scope_ref = Some("x".repeat(MAX_SCOPE_REF_CHARS + 1));
        let error = create_voice_term(&mut conn, &term, 10).expect_err("oversized scope ref");
        assert!(error.to_string().contains("scope_ref"));

        term.scope_ref = Some("com.example\u{0000}app".into());
        let error = create_voice_term(&mut conn, &term, 10).expect_err("control scope ref");
        assert!(error.to_string().contains("scope_ref"));
    }

    #[test]
    fn aliases_use_nfkc_case_and_whitespace_normalization_for_deduplication() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let stored = create_voice_term(
            &mut conn,
            &input("Café", &["cafe\u{301}", " CAFÉ ", "Ｆｉｇｍａ", "Figma"]),
            10,
        )
        .expect("store term");
        assert_eq!(stored.aliases, vec!["cafe\u{301}", "Ｆｉｇｍａ"]);
    }

    #[test]
    fn locale_is_normalized_on_write_and_malformed_values_are_rejected() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let mut term = input("Figma", &["fig ma"]);
        term.locale = Some("en_us".into());
        let stored = create_voice_term(&mut conn, &term, 10).expect("store normalized locale");
        assert_eq!(stored.locale.as_deref(), Some("en-US"));

        term.locale = Some("not a locale".into());
        let error = update_voice_term(&mut conn, stored.id, &term, 20)
            .expect_err("malformed locale must not be persisted");
        assert!(error.to_string().contains("locale must"));
    }

    #[test]
    fn rejects_aliases_above_dictionary_bound() {
        let mut conn = crate::open_in_memory().expect("migrated database");
        let aliases = (0..MAX_ALIASES_PER_TERM)
            .map(|index| format!("alias-{index}"))
            .collect::<Vec<_>>();
        let term = NewVoiceTerm {
            canonical: "Figma".into(),
            aliases,
            locale: None,
            scope: None,
            scope_ref: None,
            priority: None,
            enabled: true,
        };
        let error = create_voice_term(&mut conn, &term, 10).expect_err("too many aliases");
        assert!(error.to_string().contains("at most 16 aliases"));
    }
}
