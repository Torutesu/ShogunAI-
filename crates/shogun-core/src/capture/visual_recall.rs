//! Visual recall settings (issue #107, decision B: OCR-then-discard).
//!
//! When enabled, the macOS adapter may capture a focused window into RAM, run on-device OCR
//! (Apple Vision), persist **text + provenance** only, and discard pixels immediately. Default
//! is **off** — same opt-in posture as meeting notes (FR-MT-01).

/// Persisted visual-recall preference. Default off; serde defaults keep partial files safe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// Master switch. Off means no screen pixels are read and no OCR runs.
    #[serde(default)]
    pub enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off() {
        assert!(!Settings::default().enabled);
    }

    #[test]
    fn absent_json_field_reads_as_off() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.enabled);
    }
}
