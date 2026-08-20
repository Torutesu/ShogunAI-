//! macOS Keychain token store (invariant 7: secrets never in a file/DB/log).
//!
//! One generic-password entry per service holding the serialized [`crate::oauth::TokenSet`] (access +
//! refresh + expiry), keyed `"<source>-tokenset"` (e.g. `gmail-tokenset`) under the SHOGUN Keychain
//! service ([`keychain_store::SERVICE`]). Read + written only by [`crate::token::TokenManager`].
//!
//! macOS-only (`security-framework`) and — unlike the network transport — NOT behind the `live`
//! feature: it has no network dependency, so it type-checks for the Apple target without a TLS/C
//! toolchain.
use shogun_mcp::scope::Service;

use crate::keychain_store;
use crate::oauth::TokenSet;
use crate::token::{self, TokenStore};

/// A Keychain-backed [`TokenStore`].
pub struct KeychainTokenStore;

impl KeychainTokenStore {
    /// `keychain_service` is ignored — all secrets use [`keychain_store::SERVICE`].
    pub fn new(_keychain_service: impl Into<String>) -> Self {
        Self
    }

    fn account(service: Service) -> String {
        format!("{}-tokenset", service.source_str())
    }
}

impl TokenStore for KeychainTokenStore {
    fn load(&self, service: Service) -> Option<TokenSet> {
        let account = Self::account(service);
        let bytes = keychain_store::get_generic_secret(&account).ok()?;
        let blob = String::from_utf8(bytes).ok()?;
        token::deserialize(&blob).ok()
    }

    fn save(&self, service: Service, tokens: &TokenSet) -> Result<(), String> {
        let account = Self::account(service);
        let blob = token::serialize(tokens)?;
        keychain_store::set_generic_secret(&account, blob.as_bytes())
            .map_err(|_| format!("keychain write failed for {}", service.source_str()))
    }

    fn delete(&self, service: Service) -> Result<(), String> {
        let account = Self::account(service);
        keychain_store::delete_generic_secret(&account)
            .map_err(|_| format!("keychain delete failed for {}", service.source_str()))
    }
}
