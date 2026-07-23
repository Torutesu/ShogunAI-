//! macOS Keychain token store (invariant 7: secrets never in a file/DB/log).
//!
//! One generic-password entry per service holding the serialized [`crate::oauth::TokenSet`] (access +
//! refresh + expiry), keyed `"<source>-tokenset"` (e.g. `gmail-tokenset`) under the SHOGUN Keychain
//! service (`com.selectkk.shogun`). Read + written only by [`crate::token::TokenManager`].
//!
//! macOS-only (`security-framework`) and — unlike the network transport — NOT behind the `live`
//! feature: it has no network dependency, so it type-checks for the Apple target without a TLS/C
//! toolchain.
#![cfg(target_os = "macos")]

use shogun_mcp::scope::Service;

use crate::oauth::TokenSet;
use crate::token::{self, TokenStore};

/// A Keychain-backed [`TokenStore`].
pub struct KeychainTokenStore {
    keychain_service: String,
}

impl KeychainTokenStore {
    /// `keychain_service` is the Keychain "service" field, e.g. `com.selectkk.shogun`.
    pub fn new(keychain_service: impl Into<String>) -> Self {
        Self { keychain_service: keychain_service.into() }
    }

    fn account(service: Service) -> String {
        format!("{}-tokenset", service.source_str())
    }
}

impl TokenStore for KeychainTokenStore {
    fn load(&self, service: Service) -> Option<TokenSet> {
        let bytes = security_framework::passwords::get_generic_password(
            &self.keychain_service,
            &Self::account(service),
        )
        .ok()?;
        let blob = String::from_utf8(bytes).ok()?;
        token::deserialize(&blob).ok()
    }

    fn save(&self, service: Service, tokens: &TokenSet) -> Result<(), String> {
        let blob = token::serialize(tokens)?;
        security_framework::passwords::set_generic_password(
            &self.keychain_service,
            &Self::account(service),
            blob.as_bytes(),
        )
        .map_err(|_| format!("keychain write failed for {}", service.source_str()))
    }

    fn delete(&self, service: Service) -> Result<(), String> {
        security_framework::passwords::delete_generic_password(
            &self.keychain_service,
            &Self::account(service),
        )
        .map_err(|_| format!("keychain delete failed for {}", service.source_str()))
    }
}
