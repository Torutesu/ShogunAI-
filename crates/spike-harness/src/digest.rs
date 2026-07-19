//! Text digest helper (spec §4.4, CLAUDE.md privacy rule).
//!
//! Captured text and window-title bodies must never reach a log, metric, or telemetry
//! sink. Where a record needs to reference some text (e.g. a cache update), it stores
//! only the UTF-8 byte length and an xxHash64 digest. This module computes that pair;
//! it does not retain the input.

use std::hash::Hasher;
use twox_hash::XxHash64;

/// Length in bytes and a stable lowercase-hex xxHash64 of `text`.
/// The input is borrowed and dropped by the caller; nothing here stores it.
pub fn text_digest(text: &str) -> (usize, String) {
    let mut h = XxHash64::with_seed(0);
    h.write(text.as_bytes());
    (text.len(), format!("{:016x}", h.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_hex16() {
        let (bytes, hash) = text_digest("hello");
        assert_eq!(bytes, 5);
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // Deterministic across calls.
        assert_eq!(text_digest("hello"), (5, hash));
    }

    #[test]
    fn distinct_text_distinct_hash() {
        assert_ne!(text_digest("a").1, text_digest("b").1);
    }

    #[test]
    fn counts_utf8_bytes_not_chars() {
        // Japanese: 3 chars, 9 UTF-8 bytes.
        let (bytes, _) = text_digest("日本語");
        assert_eq!(bytes, 9);
    }
}
