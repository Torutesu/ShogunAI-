//! Diagnostic logging that redacts before it writes (design decision ② / CLAUDE.md: logs must not
//! carry keys, tokens, emails or full URLs). Use `elog!` instead of a bare `eprintln!` anywhere a
//! message could interpolate user- or provider-derived text.

/// Redact a log line via the shared log-path redactor.
pub fn scrub(line: &str) -> String {
    shogun_redact::redact_log(line).into_owned()
}

/// `eprintln!`-shaped macro that scrubs the formatted line first.
#[macro_export]
macro_rules! elog {
    ($($arg:tt)*) => {
        eprintln!("{}", $crate::log_redact::scrub(&format!($($arg)*)))
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn scrub_masks_a_key_in_a_log_line() {
        let out = super::scrub("provider error for sk-ant-api03-abcdefghijklmnop");
        assert!(!out.contains("sk-ant-api03"), "{out}");
        assert!(out.contains("[redacted]"), "{out}");
    }

    #[test]
    fn scrub_masks_an_email_in_a_log_line() {
        let out = super::scrub("failed to notify alice@example.com");
        assert!(!out.contains("alice@example.com"), "{out}");
    }
}
