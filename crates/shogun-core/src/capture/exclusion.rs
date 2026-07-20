//! Capture exclusion policy (FR-CAP-05/06) — the privacy gate that decides, before any event
//! is generated, whether the focused app/window must not be captured at all.
//!
//! Pure decision logic over `(bundle_id, window_title)`: the macOS capture layer calls
//! [`ExclusionPolicy::is_excluded`] on every focus change and simply does not emit a
//! `capture.text` event while the answer is `Some`. Defaults (password managers, the macOS
//! auth agent) are non-removable (FR-CAP-06); users may add apps and title/URL patterns.
//!
//! Private-browsing detection is heuristic and deliberately conservative: only known browsers
//! with a recognised private-mode title marker are excluded; an unknown browser is captured
//! normally (FR-CAP-05).

use std::collections::BTreeSet;

/// Why a focus was excluded from capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionReason {
    /// A password manager (non-removable default).
    PasswordManager,
    /// The macOS authentication dialog / SecurityAgent (non-removable default).
    AuthDialog,
    /// A known browser in a private/incognito window.
    PrivateBrowsing,
    /// A user-added app (by bundle id).
    UserApp,
    /// A user-added title/URL substring pattern.
    UserPattern,
}

/// Non-removable default bundle ids (FR-CAP-05, FR-CAP-06 "cannot be deleted").
const PASSWORD_MANAGERS: &[&str] = &[
    "com.1password.1password",
    "com.agilebits.onepassword7",
    "com.agilebits.onepassword4",
    "com.bitwarden.desktop",
    "org.keepassxc.keepassxc",
    "com.dashlane.Dashlane",
    "in.sinew.Enpass-Desktop",
    "com.apple.keychainaccess",
];

/// The macOS auth agent (non-removable default).
const AUTH_AGENTS: &[&str] = &["com.apple.SecurityAgent"];

/// Known browsers whose private windows we can detect by title marker.
const KNOWN_BROWSERS: &[&str] = &[
    "com.apple.Safari",
    "com.google.Chrome",
    "com.microsoft.edgemac",
    "com.brave.Browser",
    "company.thebrowser.Browser", // Arc
    "org.mozilla.firefox",
];

/// Case-insensitive title markers that indicate a private/incognito window.
const PRIVATE_TITLE_MARKERS: &[&str] = &[
    "incognito",         // Chrome/Brave/Edge: "… (Incognito)"
    "private browsing",  // Safari/Firefox
    "inprivate",         // Edge legacy
];

/// User-configurable exclusions layered over the non-removable defaults.
#[derive(Debug, Clone, Default)]
pub struct ExclusionPolicy {
    user_bundles: BTreeSet<String>,
    user_title_patterns: Vec<String>, // lowercased substrings
}

impl ExclusionPolicy {
    /// A policy with only the non-removable defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a user app exclusion by bundle id. No-op if it is already a default (those are
    /// always on anyway).
    pub fn add_app(&mut self, bundle_id: impl Into<String>) {
        self.user_bundles.insert(bundle_id.into());
    }

    /// Remove a user app exclusion. Defaults cannot be removed (FR-CAP-06) — attempting to
    /// remove one is a no-op and returns `false`.
    pub fn remove_app(&mut self, bundle_id: &str) -> bool {
        if is_default_excluded(bundle_id) {
            return false;
        }
        self.user_bundles.remove(bundle_id)
    }

    /// Add a title/URL substring pattern (matched case-insensitively against the window title).
    pub fn add_title_pattern(&mut self, pattern: impl Into<String>) {
        let p = pattern.into().to_lowercase();
        if !p.is_empty() && !self.user_title_patterns.contains(&p) {
            self.user_title_patterns.push(p);
        }
    }

    /// Decide whether this focus must not be captured. `None` = capture allowed.
    pub fn is_excluded(&self, bundle_id: &str, window_title: Option<&str>) -> Option<ExclusionReason> {
        if PASSWORD_MANAGERS.contains(&bundle_id) {
            return Some(ExclusionReason::PasswordManager);
        }
        if AUTH_AGENTS.contains(&bundle_id) {
            return Some(ExclusionReason::AuthDialog);
        }
        if self.user_bundles.contains(bundle_id) {
            return Some(ExclusionReason::UserApp);
        }
        let title_lc = window_title.map(|t| t.to_lowercase());
        if let Some(title) = &title_lc {
            if KNOWN_BROWSERS.contains(&bundle_id)
                && PRIVATE_TITLE_MARKERS.iter().any(|m| title.contains(m))
            {
                return Some(ExclusionReason::PrivateBrowsing);
            }
            if self.user_title_patterns.iter().any(|p| title.contains(p)) {
                return Some(ExclusionReason::UserPattern);
            }
        }
        None
    }
}

/// True if `bundle_id` is one of the non-removable defaults.
pub fn is_default_excluded(bundle_id: &str) -> bool {
    PASSWORD_MANAGERS.contains(&bundle_id) || AUTH_AGENTS.contains(&bundle_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_default_password_managers_are_excluded() {
        let p = ExclusionPolicy::new();
        for pm in PASSWORD_MANAGERS {
            assert_eq!(p.is_excluded(pm, None), Some(ExclusionReason::PasswordManager), "{pm}");
        }
    }

    #[test]
    fn auth_agent_is_excluded() {
        let p = ExclusionPolicy::new();
        assert_eq!(p.is_excluded("com.apple.SecurityAgent", None), Some(ExclusionReason::AuthDialog));
    }

    #[test]
    fn ordinary_app_is_captured() {
        let p = ExclusionPolicy::new();
        assert_eq!(p.is_excluded("com.apple.Safari", Some("GitHub — Pull requests")), None);
        assert_eq!(p.is_excluded("com.microsoft.VSCode", Some("main.rs")), None);
    }

    #[test]
    fn chrome_incognito_is_excluded_but_normal_chrome_is_not() {
        let p = ExclusionPolicy::new();
        assert_eq!(
            p.is_excluded("com.google.Chrome", Some("Docs (Incognito)")),
            Some(ExclusionReason::PrivateBrowsing)
        );
        assert_eq!(p.is_excluded("com.google.Chrome", Some("Docs")), None);
    }

    #[test]
    fn safari_private_browsing_marker() {
        let p = ExclusionPolicy::new();
        assert_eq!(
            p.is_excluded("com.apple.Safari", Some("Private Browsing")),
            Some(ExclusionReason::PrivateBrowsing)
        );
    }

    #[test]
    fn unknown_browser_private_is_not_detected() {
        // An unknown browser is captured normally even if its title says "incognito"
        // (FR-CAP-05: only known browsers are judged).
        let p = ExclusionPolicy::new();
        assert_eq!(p.is_excluded("com.unknown.browser", Some("x (Incognito)")), None);
    }

    #[test]
    fn user_app_exclusion_add_and_remove() {
        let mut p = ExclusionPolicy::new();
        p.add_app("com.example.SecretApp");
        assert_eq!(p.is_excluded("com.example.SecretApp", None), Some(ExclusionReason::UserApp));
        assert!(p.remove_app("com.example.SecretApp"));
        assert_eq!(p.is_excluded("com.example.SecretApp", None), None);
    }

    #[test]
    fn default_exclusions_cannot_be_removed() {
        let mut p = ExclusionPolicy::new();
        assert!(!p.remove_app("com.1password.1password"), "a default must not be removable");
        assert_eq!(
            p.is_excluded("com.1password.1password", None),
            Some(ExclusionReason::PasswordManager)
        );
    }

    #[test]
    fn user_title_pattern_matches_case_insensitively() {
        let mut p = ExclusionPolicy::new();
        p.add_title_pattern("Salary");
        assert_eq!(
            p.is_excluded("com.apple.Numbers", Some("2026 salary review.numbers")),
            Some(ExclusionReason::UserPattern)
        );
        assert_eq!(p.is_excluded("com.apple.Numbers", Some("budget.numbers")), None);
    }
}
