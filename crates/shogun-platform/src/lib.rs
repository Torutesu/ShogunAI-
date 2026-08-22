//! OS adapters for the Windows / Linux product shell.
//!
//! The macOS app owns Keychain, AX capture, and the Notch. This crate is the matching *boundary*
//! on PC: where the process may write files, and where it may store secrets. It does not own a
//! world model, a database, or an HTTP client (FR-TR-03 / invariant 1 stay in `shogun-core`).
//!
//! Secrets never go in the app-data directory. On Windows they go to Credential Manager. On Linux
//! this slice fails closed rather than writing a plaintext file (invariant 7).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod paths;
pub mod secrets;

pub use paths::{app_data_dir, ensure_app_data_dir, PathError};
pub use secrets::{delete_secret, get_secret, secret_store_status, set_secret, SecretError, SecretSlot, SecretStoreStatus};
