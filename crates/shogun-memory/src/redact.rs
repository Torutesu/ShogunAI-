//! Secret redaction on the write path.
//!
//! Everything SHOGUN reads gets stored, and screens and AI-tool transcripts routinely contain
//! credentials — a key pasted into a terminal, a token in a config file on screen, an
//! `Authorization` header in a response. Storing those verbatim would turn the memory database
//! into a credential store, so recognisable secrets are masked *before* the row is written. The
//! database only ever holds the masked form; there is no "raw" copy to leak later.
//!
//! Two deliberate limits:
//!
//! * This is **not** a security boundary — it cannot catch a secret with no recognisable shape.
//!   It reduces exposure; encryption at rest is what actually protects the file.
//! * It is tuned to **avoid false positives**. Mangling real text would corrupt the memory the
//!   product is built on, so a pattern is only matched when it is specifically credential-shaped
//!   (a known issuer prefix, or a value that follows a key/token/password label).
//!
//! Pure and dependency-free: hand-rolled scanning rather than a regex engine, so it is exhaustively
//! testable and adds nothing to the build.

/// What replaces a matched secret. Fixed-width so the shape of the surrounding text survives.
const MASK: &str = "[redacted]";

/// Issuer prefixes that are unambiguous on sight. Each entry is the literal prefix; the run of
/// secret-ish characters that follows it is masked with it.
const ISSUER_PREFIXES: &[&str] = &[
    "sk-ant-",         // Anthropic
    "sk-",             // OpenAI and friends
    "ghp_",            // GitHub personal access token
    "gho_",            // GitHub OAuth
    "ghu_",            //
    "ghs_",            // GitHub server-to-server
    "ghr_",            //
    "github_pat_",     // GitHub fine-grained PAT
    "xoxb-",           // Slack bot
    "xoxp-",           // Slack user
    "xoxa-",           //
    "xoxs-",           //
    "AKIA",            // AWS access key id
    "ASIA",            // AWS temporary
    "AIza",            // Google API key
    "ya29.",           // Google OAuth access token
    "glpat-",          // GitLab PAT
    "sq0atp-",         // Square
    "sq0csp-",         //
    "shpat_",          // Shopify
    "SG.",             // SendGrid
    "GOCSPX-",         // Google OAuth client secret
];

/// Labels whose following value is a secret (`api_key: …`, `password=…`, `Authorization: Bearer …`).
const LABELS: &[&str] = &[
    "api_key", "api-key", "apikey", "access_token", "access-token", "accesstoken",
    "refresh_token", "refresh-token", "client_secret", "client-secret", "secret_key",
    "secret-key", "password", "passwd", "authorization", "auth_token", "auth-token",
    "private_key", "private-key", "token", "secret",
];

/// Shortest run that counts as a secret value. Below this it is more likely a placeholder or a
/// word than a credential, and masking it would do more harm than good.
const MIN_SECRET_LEN: usize = 12;

/// Characters that can appear inside a token value.
fn is_secret_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '/' | '=' | '~')
}

/// Mask recognisable secrets in `text`.
///
/// Returns the input unchanged (no allocation) when nothing matched, which is the common case for
/// ordinary captured prose.
pub fn redact(text: &str) -> std::borrow::Cow<'_, str> {
    // Cheap pre-check so the ordinary path costs one scan and no allocation.
    if !might_contain_secret(text) {
        return std::borrow::Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        if !text.is_char_boundary(i) {
            // Never split a codepoint: copy the byte through and move on.
            out.push_str(&text[i..i + 1]);
            i += 1;
            continue;
        }
        let rest = &text[i..];

        // 1. A known issuer prefix: mask the prefix and the value that follows it.
        if let Some(p) = ISSUER_PREFIXES.iter().find(|p| rest.starts_with(**p)) {
            let value_len = run_len(&rest[p.len()..]);
            if p.len() + value_len >= MIN_SECRET_LEN {
                out.push_str(MASK);
                i += p.len() + value_len;
                continue;
            }
        }

        // 2. A labelled value: `token: …`, `password=…`, `Authorization: Bearer …`.
        if let Some(consumed) = match_labelled(rest) {
            out.push_str(&rest[..consumed.label_end]);
            out.push_str(MASK);
            i += consumed.total;
            continue;
        }

        // Not a match: copy this character.
        let ch_len = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&rest[..ch_len]);
        i += ch_len;
        let _ = bytes;
    }
    std::borrow::Cow::Owned(out)
}

/// True when `text` contains anything worth a full scan.
fn might_contain_secret(text: &str) -> bool {
    if ISSUER_PREFIXES.iter().any(|p| text.contains(p)) {
        return true;
    }
    // Labels are matched case-insensitively, so pre-check the same way.
    let lower = text.to_ascii_lowercase();
    LABELS.iter().any(|l| lower.contains(l))
}

/// How long the run of secret-ish characters at the start of `s` is.
fn run_len(s: &str) -> usize {
    s.char_indices()
        .find(|(_, c)| !is_secret_char(*c))
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

struct Labelled {
    /// Bytes up to and including the separator + optional `Bearer `, copied through verbatim.
    label_end: usize,
    /// Total bytes consumed (label + separator + the masked value).
    total: usize,
}

/// Match `label` `sep` `value` at the start of `s`, where sep is `:` or `=` with optional spaces
/// and quotes. Returns `None` unless the value is long enough to be a real credential.
fn match_labelled(s: &str) -> Option<Labelled> {
    let lower = s.to_ascii_lowercase();
    let label = LABELS.iter().find(|l| lower.starts_with(**l))?;
    let mut p = label.len();

    // Optional closing quote of a quoted key ("api_key": …) then the separator.
    let b = s.as_bytes();
    while p < b.len() && (b[p] == b'"' || b[p] == b'\'' || b[p] == b' ') {
        p += 1;
    }
    if p >= b.len() || (b[p] != b':' && b[p] != b'=') {
        return None;
    }
    p += 1;
    while p < b.len() && (b[p] == b' ' || b[p] == b'"' || b[p] == b'\'') {
        p += 1;
    }
    // `Authorization: Bearer <token>` — keep the scheme, mask the token.
    for scheme in ["bearer ", "basic ", "token "] {
        if lower[p.min(lower.len())..].starts_with(scheme) {
            p += scheme.len();
            break;
        }
    }
    let value_len = run_len(&s[p..]);
    if value_len < MIN_SECRET_LEN {
        return None;
    }
    Some(Labelled { label_end: p, total: p + value_len })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> String {
        redact(s).into_owned()
    }

    #[test]
    fn ordinary_text_is_untouched_and_not_reallocated() {
        let text = "Weekly sync notes. I'll send Alice the deck tomorrow. Waiting on legal.";
        assert!(matches!(redact(text), std::borrow::Cow::Borrowed(_)), "no copy for clean text");
        assert_eq!(r(text), text);
    }

    #[test]
    fn issuer_prefixed_keys_are_masked() {
        assert_eq!(r("key sk-ant-api03-abcdefghijklmnop here"), "key [redacted] here");
        assert_eq!(r("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"), "[redacted]");
        assert_eq!(r("use AKIAIOSFODNN7EXAMPLE now"), "use [redacted] now");
        assert_eq!(r("xoxb-123456789012-abcdefghijkl"), "[redacted]");
        assert_eq!(r("GOCSPX-abcdefghijklmnopqrst"), "[redacted]");
    }

    #[test]
    fn labelled_values_are_masked_but_the_label_survives() {
        assert_eq!(r("api_key: abcdefghijklmnopqrst"), "api_key: [redacted]");
        assert_eq!(r("password=hunter2hunter2hunter2"), "password=[redacted]");
        assert_eq!(r(r#""client_secret": "abcdefghijklmnop""#), r#""client_secret": "[redacted]""#);
    }

    #[test]
    fn an_authorization_header_keeps_its_scheme() {
        assert_eq!(
            r("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
            "Authorization: Bearer [redacted]"
        );
    }

    #[test]
    fn short_values_are_left_alone_so_real_text_is_not_mangled() {
        // A "token" in prose, and a placeholder — masking these would corrupt the memory.
        assert_eq!(r("the token: abc"), "the token: abc");
        assert_eq!(r("password: TODO"), "password: TODO");
        assert_eq!(r("sk-short"), "sk-short");
    }

    #[test]
    fn words_containing_a_label_are_not_treated_as_one() {
        // No separator follows, so nothing is masked.
        let s = "tokenising the passwordless flow takes a while honestly";
        assert_eq!(r(s), s);
    }

    #[test]
    fn multi_byte_text_survives_redaction_intact() {
        let s = "資料はこちら api_key: abcdefghijklmnopqrst です。日本語も壊れない。";
        let got = r(s);
        assert!(got.contains("資料はこちら"), "prefix intact: {got}");
        assert!(got.contains("です。日本語も壊れない。"), "suffix intact: {got}");
        assert!(got.contains("[redacted]"));
        assert!(!got.contains("abcdefghijklmnopqrst"));
    }

    #[test]
    fn several_secrets_in_one_blob_are_all_masked() {
        let got = r("first ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345 then api_key: zyxwvutsrqponmlkjihg end");
        assert!(!got.contains("ghp_ABCDEF"), "{got}");
        assert!(!got.contains("zyxwvutsrqponmlkjihg"), "{got}");
        assert!(got.starts_with("first ") && got.ends_with(" end"), "{got}");
    }
}
