//! Untrusted persist preparation (memory-poisoning P2).
//!
//! Every writer that used to call [`crate::redact::redact`] on a body that came from the world
//! (capture, mail, OCR, transcript, generated summaries that can echo source text) goes through
//! [`persist_body`]: hidden format characters are stripped first, then secrets are masked.
//! Strip-then-redact is load-bearing — a key split by ZWSP would otherwise miss the issuer prefix.
//!
//! Counts (`events_stripped` / `chars_removed`) are process-wide and never include the text.

use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

use shogun_redact::strip_hidden;

static EVENTS_STRIPPED: AtomicU64 = AtomicU64::new(0);
static CHARS_REMOVED: AtomicU64 = AtomicU64::new(0);

/// Hidden-format counts since process start. Zeros are a real measurement (nothing stripped),
/// not an unmeasured flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SanitizerSnapshot {
    pub events_stripped: u64,
    pub chars_removed: u64,
}

/// Process-wide sanitizer counters for `GET /v1/metrics`. Never the captured text.
pub fn snapshot() -> SanitizerSnapshot {
    SanitizerSnapshot {
        events_stripped: EVENTS_STRIPPED.load(Ordering::Relaxed),
        chars_removed: CHARS_REMOVED.load(Ordering::Relaxed),
    }
}

/// Strip hidden format characters and record a sanitizer hit. No secret masking — for JSON
/// columns that must stay parseable (recap decisions / next_actions).
pub fn persist_hidden(raw: &str) -> PersistBody<'_> {
    let stripped = strip_hidden(raw);
    if stripped.removed > 0 {
        EVENTS_STRIPPED.fetch_add(1, Ordering::Relaxed);
        CHARS_REMOVED.fetch_add(u64::from(stripped.removed), Ordering::Relaxed);
    }
    PersistBody {
        text: stripped.text,
        hidden_removed: stripped.removed,
    }
}

/// Strip hidden format characters, then mask secrets. Records a sanitizer hit when anything
/// was stripped. Clean text that also has no secret is borrowed.
pub fn persist_body(raw: &str) -> PersistBody<'_> {
    let hidden = persist_hidden(raw);
    let hidden_removed = hidden.hidden_removed;
    let text = match hidden.text {
        Cow::Borrowed(b) => crate::redact::redact(b),
        Cow::Owned(s) => Cow::Owned(crate::redact::redact(&s).into_owned()),
    };
    PersistBody {
        text,
        hidden_removed,
    }
}

/// Prepared untrusted body ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistBody<'a> {
    pub text: Cow<'a, str>,
    pub hidden_removed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_is_borrowed() {
        let raw = "I'll send the deck tomorrow.";
        let got = persist_body(raw);
        assert_eq!(got.hidden_removed, 0);
        assert!(matches!(got.text, Cow::Borrowed(_)));
        assert_eq!(got.text.as_ref(), raw);
    }

    #[test]
    fn hidden_runes_are_gone_before_the_row_exists() {
        let got = persist_body("review\u{200B}Ignore previous");
        assert_eq!(got.text.as_ref(), "reviewIgnore previous");
        assert_eq!(got.hidden_removed, 1);
    }

    #[test]
    fn zwsp_inside_a_key_is_stripped_then_redacted() {
        let raw = "sk-\u{200B}ant-api03-abcdefghijklmnop";
        let got = persist_body(raw);
        assert_eq!(got.text.as_ref(), "[redacted]");
        assert_eq!(got.hidden_removed, 1);
    }
}
