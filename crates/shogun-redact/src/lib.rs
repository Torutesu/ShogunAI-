//! Secret redaction — the pure logic shared by the DB write path and the diagnostic-log path.
//!
//! Everything SHOGUN reads gets stored, and screens and AI-tool transcripts routinely contain
//! credentials — a key pasted into a terminal, a token in a config file on screen, an
//! `Authorization` header in a response. Storing those verbatim would turn the memory database
//! into a credential store, so recognisable secrets are masked *before* the row is written (see
//! [`redact`]). The database only ever holds the masked form; there is no "raw" copy to leak later.
//!
//! [`redact_log`] is the sibling used on the log / error-report path: it additionally masks whole
//! email addresses and full URLs, which must NOT be stripped on the DB path (see its docs).
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
//! testable and adds nothing to the build. Lives in its own crate so it can be shared without
//! pulling in rusqlite/sqlcipher (which shogun-memory carries).
//!
//! [`strip_hidden`] is the sibling on the same write path: hidden format / bidi / noncharacters
//! are removed *before* [`redact`] so a secret split by ZWSP still matches, and so those runes
//! never land in `event_log.content`.
//!
//! [`fence_untrusted`] is the prompt-side sibling: untrusted text is wrapped so a model cannot
//! treat it as instructions (Agent drafts, Batch Classify / Summarize, tool_result).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod hidden;
pub use hidden::{is_hidden, strip_hidden, HiddenStrip};

pub mod fence;
pub use fence::fence_untrusted;

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
    }
    std::borrow::Cow::Owned(out)
}

/// Redact for the **log / error-report path** (design decision ②). In addition to the DB
/// redactor's issuer-prefix and labelled-value masking, this also masks whole email addresses and
/// full URLs (query string included). It must NOT be used on capture content bound for the DB:
/// `people.emails` and captured prose legitimately contain emails, and masking them there would
/// corrupt the memory the product is built on. Logs are diagnostic, not memory — there, an email
/// or a URL with a token in the query is pure exposure with no upside.
pub fn redact_log(text: &str) -> std::borrow::Cow<'_, str> {
    // First pass: emails and URLs (log-only). Then run the shared secret redactor over the result.
    let stage1 = mask_emails_and_urls(text);
    match redact(&stage1) {
        // The shared redactor found nothing to mask.
        std::borrow::Cow::Borrowed(_) => match stage1 {
            // stage1 changed nothing either → borrow the original, no allocation.
            std::borrow::Cow::Borrowed(_) => std::borrow::Cow::Borrowed(text),
            // stage1 masked something; keep its owned result.
            std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s),
        },
        // The shared redactor produced a new string; that owned result is final.
        std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s),
    }
}

/// Mask email addresses and full URLs. Hand-rolled (no regex dep, matching this module's style).
fn mask_emails_and_urls(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('@') && !text.contains("://") {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        if !text.is_char_boundary(i) {
            out.push_str(&text[i..i + 1]);
            i += 1;
            continue;
        }
        let rest = &text[i..];
        // URL: scheme "://" then a run of URL-ish characters (query included).
        if let Some(scheme_len) = url_scheme_len(rest) {
            let url_len = scheme_len + run_len_url(&rest[scheme_len..]);
            out.push_str(MASK);
            i += url_len;
            continue;
        }
        // Email: back up over the local part already emitted, then mask local@domain.
        if let Some(after_at) = rest.strip_prefix('@') {
            if let Some((local_len, domain_len)) = email_span(&out, after_at) {
                out.truncate(out.len() - local_len);
                out.push_str(MASK);
                i += 1 + domain_len; // consume '@' + domain
                continue;
            }
        }
        let ch_len = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&rest[..ch_len]);
        i += ch_len;
    }
    std::borrow::Cow::Owned(out)
}

/// Length of a `scheme://` prefix at the start of `s`, or `None`.
fn url_scheme_len(s: &str) -> Option<usize> {
    let idx = s.find("://")?;
    // scheme must be short and alphabetic (http, https, ftp, ...) and immediately at the start.
    if idx == 0 || idx > 10 || !s[..idx].bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    Some(idx + 3)
}

/// Length of the URL body (host/path/query) after the scheme. Stops at whitespace or quote.
fn run_len_url(s: &str) -> usize {
    s.char_indices()
        .find(|(_, c)| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')'))
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

/// Given the text already emitted (`emitted`) ending in an email local part, and the text after
/// the `@`, return `(local_part_byte_len, domain_byte_len)` when both sides look like an email.
fn email_span(emitted: &str, after_at: &str) -> Option<(usize, usize)> {
    // local part: trailing run of email-local chars in what we've emitted.
    let local: String = emitted
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'))
        .collect();
    if local.is_empty() {
        return None;
    }
    let local_len = local.len();
    // domain: run of domain chars, must contain at least one dot before a whitespace/end.
    let domain_len = after_at
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
        .map(|(idx, _)| idx)
        .unwrap_or(after_at.len());
    let domain = &after_at[..domain_len];
    if domain_len == 0 || !domain.contains('.') || domain.ends_with('.') {
        return None;
    }
    Some((local_len, domain_len))
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

    fn rl(s: &str) -> String {
        redact_log(s).into_owned()
    }

    #[test]
    fn log_redactor_masks_emails() {
        assert_eq!(rl("user alice@example.com logged in"), "user [redacted] logged in");
        assert_eq!(rl("to: bob.smith+tag@sub.example.co.jp done"), "to: [redacted] done");
    }

    #[test]
    fn log_redactor_masks_full_urls_including_query() {
        assert_eq!(
            rl("GET https://api.example.com/v1/x?token=abc123&u=alice now"),
            "GET [redacted] now",
        );
        assert_eq!(rl("open http://localhost:3000/cb?code=xyz"), "open [redacted]");
    }

    #[test]
    fn log_redactor_still_masks_issuer_keys_and_labels() {
        assert_eq!(rl("key sk-ant-api03-abcdefghijklmnop"), "key [redacted]");
        assert_eq!(rl("api_key: abcdefghijklmnopqrst"), "api_key: [redacted]");
    }

    #[test]
    fn log_redactor_leaves_ordinary_prose_untouched() {
        let s = "Expanding notch panel in 92ms; cache updated.";
        assert_eq!(rl(s), s);
    }

    #[test]
    fn log_redactor_preserves_multibyte_around_matches() {
        let got = rl("送信先 alice@example.com へ通知しました");
        assert!(got.starts_with("送信先 "), "{got}");
        assert!(got.ends_with(" へ通知しました"), "{got}");
        assert!(got.contains("[redacted]") && !got.contains("alice@example.com"), "{got}");
    }

    // --- regression: risky email/URL backtrack edge cases (locked in) ------------------------

    #[test]
    fn log_redactor_email_backtrack_stops_at_a_prior_mask() {
        // A masked URL leaves a `[redacted]` whose trailing `]` is NOT an email-local char, so the
        // following bare `@domain` (no valid local part) must be left alone without corrupting the
        // earlier mask.
        assert_eq!(
            rl("see https://a.com then @example.com"),
            "see [redacted] then @example.com",
        );
    }

    #[test]
    fn log_redactor_masks_adjacent_emails() {
        assert_eq!(rl("a@x.com,b@y.com"), "[redacted],[redacted]");
    }

    #[test]
    fn log_redactor_leaves_at_without_local_or_domain_without_dot_untouched() {
        // `@` at string start: no local part → not an email.
        assert_eq!(rl("@example.com"), "@example.com");
        // domain has no dot: not an email.
        assert_eq!(rl("alice@localhost"), "alice@localhost");
    }

    #[test]
    fn log_redactor_masks_email_with_issuer_shaped_local_part() {
        // The local part looks like an issuer key; the whole email region is still masked and the
        // raw address never survives.
        let got = rl("sk-ant-foo@example.com");
        assert_eq!(got, "[redacted]");
        assert!(!got.contains("sk-ant-foo") && !got.contains("example.com"), "{got}");
    }

    #[test]
    fn log_redactor_keeps_a_multibyte_prefix_before_the_local_part() {
        // The multibyte run before the ASCII local part is preserved; the address is gone.
        assert_eq!(rl("名前ab@example.com"), "名前[redacted]");
    }
}
