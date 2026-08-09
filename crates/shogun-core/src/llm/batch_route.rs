//! Relay-vs-direct routing for the Batch lane (docs/batch-relay-design.md §7, E-38).
//!
//! The interim direct-Anthropic path (a raw operator key in the developer's own Keychain) must
//! never ship: a distributed binary that can reach Anthropic directly is the rejected design of
//! §2.1 — the key becomes extractable and the server-side spend cap unenforceable. This module
//! makes "must never ship" a compile-time property instead of a review item:
//!
//! - [`BatchRoute::DirectAnthropic`] **exists only under `cfg(debug_assertions)`**. Release
//!   code that names it does not compile, so there is no release code path that constructs the
//!   direct client. (`scripts/check-secret-exposure.py` additionally guards the desktop crate's
//!   references textually.)
//! - Even in debug builds the direct route requires an explicit env opt-in
//!   ([`DEV_DIRECT_ENV`]`=1`), so a developer build defaults to the relay too.
//!
//! [`batch_route`] is a pure function — the caller reads the env var and passes it in — so the
//! decision table is exhaustively testable.

/// The env var a developer sets to `1` to opt into the direct-Anthropic Batch path
/// (debug builds only; ignored — and unusable — in release).
pub const DEV_DIRECT_ENV: &str = "SHOGUN_DEV_DIRECT_ANTHROPIC";

/// Which client the Batch lane constructs tonight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchRoute {
    /// The shipping path: license token → Select-operated relay ([`super::relay`]).
    Relay,
    /// The development-only path: a raw Anthropic key in the developer's Keychain, hitting the
    /// Batch API directly. The variant does not exist in release builds — matching on it or
    /// constructing it outside `cfg(debug_assertions)` is a compile error, which is the
    /// enforcement the design doc's §7 warning asks for.
    #[cfg(debug_assertions)]
    DirectAnthropic,
}

/// Decide the Batch route from the dev opt-in (`dev_direct_opt_in` is the value of
/// [`DEV_DIRECT_ENV`], read by the caller). Release builds return [`BatchRoute::Relay`]
/// unconditionally — the direct variant is not even representable there.
#[must_use]
pub fn batch_route(dev_direct_opt_in: Option<&str>) -> BatchRoute {
    #[cfg(debug_assertions)]
    if dev_direct_opt_in == Some("1") {
        return BatchRoute::DirectAnthropic;
    }
    // Release: the opt-in cannot select a route that does not exist.
    let _ = dev_direct_opt_in;
    BatchRoute::Relay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_relay_even_in_debug_builds() {
        assert_eq!(batch_route(None), BatchRoute::Relay);
        assert_eq!(batch_route(Some("")), BatchRoute::Relay);
        assert_eq!(batch_route(Some("0")), BatchRoute::Relay);
        assert_eq!(batch_route(Some("true")), BatchRoute::Relay, "only the literal \"1\" opts in");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn a_debug_build_can_opt_into_the_direct_path_explicitly() {
        assert_eq!(batch_route(Some("1")), BatchRoute::DirectAnthropic);
    }

    /// The release half of the guarantee, runnable with `cargo test --release`: the opt-in is
    /// inert, and — stronger — `BatchRoute::DirectAnthropic` does not exist to return. The
    /// following must NOT compile in a release build (and is the property the desktop's
    /// `dream.rs` leans on):
    ///     let _ = BatchRoute::DirectAnthropic;
    #[cfg(not(debug_assertions))]
    #[test]
    fn a_release_build_cannot_be_opted_into_the_direct_path() {
        assert_eq!(batch_route(Some("1")), BatchRoute::Relay);
    }
}
