//! Pure helpers for the desktop "Connect" flow (plan B-4): env-driven Wave-1 read enablement and
//! the typed outcome of an interactive OAuth connect attempt.
//!
//! Linux-testable, no effects. The desktop adapter (`apps/desktop/src-tauri/src/connectors.rs`)
//! reads the env vars and maps a [`ConnectError`] onto the FR-INT-06/07 state machine:
//! - a failed *attempt* ([`ConnectError::marks_amber`] = true — denial, timeout, exchange or
//!   persist failure) applies `ConnEvent::ConnectFailed` → amber, with the reauth affordance;
//! - a precondition problem (missing OAuth client config, listener bind failure) leaves the
//!   service Disconnected, so the Connect button itself stays as the retry affordance.
//!
//! Nothing here ever carries a token, code, or verifier — reasons are content-free strings.

use std::fmt;

use shogun_mcp::scope::Service;

/// Env var naming the Google OAuth "Desktop app" client id (docs/oauth-client-setup.md §1-6).
pub const GOOGLE_CLIENT_ID_ENV: &str = "SHOGUN_GOOGLE_CLIENT_ID";
/// Env var naming the Google OAuth client secret (a Desktop-app secret is non-confidential but
/// still never committed; optional for a pure-PKCE client).
pub const GOOGLE_CLIENT_SECRET_ENV: &str = "SHOGUN_GOOGLE_CLIENT_SECRET";
/// Env opt-in letting on-device live verification enable the Calendar / Drive first-layer read
/// path without a rebuild, e.g. `SHOGUN_ENABLE_WAVE1_READ=calendar,drive`. Unset or empty keeps
/// the shipped default: the wired transport serves Gmail only, Calendar/Drive stay "Coming soon".
pub const WAVE1_READ_ENV: &str = "SHOGUN_ENABLE_WAVE1_READ";

/// Parse the [`WAVE1_READ_ENV`] opt-in list into the extra Wave-1 services it enables.
///
/// Accepts comma-separated tokens, case-insensitive, whitespace-tolerant. Both the plain name and
/// the `source_str` id are accepted (`calendar` / `gcal`, `drive` / `gdrive`). Unknown tokens are
/// ignored (never a panic — a typo must not take the connector runtime down), `gmail` is ignored
/// because Gmail is always served. The result is deduplicated.
pub fn parse_wave1_read_optin(raw: Option<&str>) -> Vec<Service> {
    let mut out = Vec::new();
    let Some(raw) = raw else { return out };
    for token in raw.split(',') {
        let service = match token.trim().to_ascii_lowercase().as_str() {
            "calendar" | "gcal" => Some(Service::GoogleCalendar),
            "drive" | "gdrive" => Some(Service::GoogleDrive),
            _ => None,
        };
        if let Some(s) = service {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// Whether the wired desktop transport can actually serve `service`: Gmail always (the Composio
/// read transport), plus whatever [`parse_wave1_read_optin`] enabled (the first-layer MCP client).
/// Everything else is presented as "Coming soon" rather than a Connect button that can only end in
/// a false amber.
pub fn transport_serves(service: Service, extra_enabled: &[Service]) -> bool {
    service == Service::Gmail || extra_enabled.contains(&service)
}

/// Why an interactive OAuth connect attempt did not end in a connected service. Reasons are short
/// and content-free (no token, code, or verifier ever appears here — invariant 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// No OAuth client is configured for this service (env vars absent). A setup problem, not a
    /// failed attempt — the service stays Disconnected.
    MissingClientConfig { service: &'static str },
    /// The loopback listener could not be bound / used.
    ListenerBind(String),
    /// The system browser could not be opened for the consent page.
    BrowserOpen(String),
    /// The user declined consent on the provider page (`error=access_denied` redirect).
    Denied,
    /// No redirect arrived before the deadline (browser closed / user walked away).
    Timeout,
    /// The redirect was malformed or failed the anti-CSRF state check.
    BadRedirect(String),
    /// The code→token exchange (or token-response parse) failed.
    Exchange(String),
    /// Tokens were obtained but could not be persisted to the Keychain — treated as a failed
    /// attempt (nothing was stored; there is no half-connected state).
    Persist(String),
    /// An internal step failed (entropy, URL building). Should not happen in practice.
    Internal(String),
}

impl ConnectError {
    /// Whether this failure should turn the service amber (`ConnEvent::ConnectFailed`,
    /// FR-INT-06) — true for a real attempt that failed mid-flight. False for precondition
    /// problems, which leave the service Disconnected with the Connect button as the retry path.
    pub fn marks_amber(&self) -> bool {
        !matches!(
            self,
            ConnectError::MissingClientConfig { .. } | ConnectError::ListenerBind(_)
        )
    }
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectError::MissingClientConfig { service } => write!(
                f,
                "{service}: OAuth client is not configured — set {GOOGLE_CLIENT_ID_ENV} (and \
                 {GOOGLE_CLIENT_SECRET_ENV}) in the app's environment, then retry Connect."
            ),
            ConnectError::ListenerBind(reason) => write!(
                f,
                "Could not open the local sign-in listener ({reason}). Retry Connect."
            ),
            ConnectError::BrowserOpen(reason) => write!(
                f,
                "Could not open the browser for sign-in ({reason}). Retry Connect."
            ),
            ConnectError::Denied => {
                write!(f, "Sign-in was declined in the browser. Retry Connect when ready.")
            }
            ConnectError::Timeout => write!(
                f,
                "Sign-in timed out before the browser returned. Retry Connect."
            ),
            ConnectError::BadRedirect(reason) => {
                write!(f, "Sign-in was interrupted ({reason}). Retry Connect.")
            }
            ConnectError::Exchange(reason) => {
                write!(f, "Could not finish sign-in ({reason}). Retry Connect.")
            }
            ConnectError::Persist(reason) => write!(
                f,
                "Signed in, but storing the connection failed ({reason}). Retry Connect."
            ),
            ConnectError::Internal(reason) => {
                write!(f, "Connect failed ({reason}). Retry Connect.")
            }
        }
    }
}

/// True when a [`crate::oauth::parse_redirect`] error means the user denied consent (the provider
/// redirected back with `error=…`), as opposed to a stray or malformed request (e.g. the browser's
/// favicon probe hitting the loopback port) that the listener should answer and keep waiting past.
pub fn redirect_error_is_denial(err: &str) -> bool {
    err.starts_with("authorization denied")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optin_default_is_empty_gmail_only() {
        assert!(parse_wave1_read_optin(None).is_empty());
        assert!(parse_wave1_read_optin(Some("")).is_empty());
        // Default: only Gmail is served.
        assert!(transport_serves(Service::Gmail, &[]));
        assert!(!transport_serves(Service::GoogleCalendar, &[]));
        assert!(!transport_serves(Service::GoogleDrive, &[]));
        assert!(!transport_serves(Service::Slack, &[]));
    }

    #[test]
    fn optin_parses_names_and_source_ids_case_insensitively() {
        assert_eq!(
            parse_wave1_read_optin(Some("calendar,drive")),
            vec![Service::GoogleCalendar, Service::GoogleDrive]
        );
        assert_eq!(
            parse_wave1_read_optin(Some(" GCal , GDRIVE ")),
            vec![Service::GoogleCalendar, Service::GoogleDrive]
        );
        assert_eq!(parse_wave1_read_optin(Some("drive")), vec![Service::GoogleDrive]);
    }

    #[test]
    fn optin_ignores_unknown_gmail_and_duplicates() {
        // gmail is always on, typos never panic, dupes collapse.
        assert_eq!(
            parse_wave1_read_optin(Some("gmail,calendar,calendar,slack,wat")),
            vec![Service::GoogleCalendar]
        );
    }

    #[test]
    fn optin_flips_transport_serves_without_touching_others() {
        let extra = parse_wave1_read_optin(Some("calendar"));
        assert!(transport_serves(Service::Gmail, &extra));
        assert!(transport_serves(Service::GoogleCalendar, &extra));
        assert!(!transport_serves(Service::GoogleDrive, &extra));
        assert!(!transport_serves(Service::Notion, &extra));
    }

    #[test]
    fn amber_mapping_matches_the_state_machine_contract() {
        // Real failed attempts → amber (ConnEvent::ConnectFailed upstream).
        assert!(ConnectError::Denied.marks_amber());
        assert!(ConnectError::Timeout.marks_amber());
        assert!(ConnectError::BrowserOpen("x".into()).marks_amber());
        assert!(ConnectError::BadRedirect("x".into()).marks_amber());
        assert!(ConnectError::Exchange("x".into()).marks_amber());
        assert!(ConnectError::Persist("x".into()).marks_amber());
        assert!(ConnectError::Internal("x".into()).marks_amber());
        // Precondition problems → stay Disconnected (Connect button is the retry affordance).
        assert!(!ConnectError::MissingClientConfig { service: "gcal" }.marks_amber());
        assert!(!ConnectError::ListenerBind("x".into()).marks_amber());
    }

    #[test]
    fn display_is_actionable_and_content_free() {
        let msg = ConnectError::MissingClientConfig { service: "gcal" }.to_string();
        assert!(msg.contains("SHOGUN_GOOGLE_CLIENT_ID"), "{msg}");
        assert!(msg.contains("Retry") || msg.contains("retry"), "{msg}");
        for e in [
            ConnectError::Denied,
            ConnectError::Timeout,
            ConnectError::ListenerBind("addr in use".into()),
            ConnectError::Exchange("token endpoint http 400".into()),
        ] {
            let m = e.to_string();
            assert!(m.contains("Retry Connect"), "retry affordance missing: {m}");
        }
    }

    #[test]
    fn denial_classification_pins_parse_redirect_output() {
        // Tie the classifier to the real parse_redirect error strings, so they cannot drift apart.
        let denied =
            crate::oauth::parse_redirect("GET /callback?error=access_denied HTTP/1.1").unwrap_err();
        assert!(redirect_error_is_denial(&denied));
        let stray = crate::oauth::parse_redirect("GET /favicon.ico HTTP/1.1").unwrap_err();
        assert!(!redirect_error_is_denial(&stray));
    }
}
