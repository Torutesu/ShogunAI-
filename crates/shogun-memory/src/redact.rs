//! Secret redaction — the pure logic lives in the dependency-free `shogun-redact` crate so it can
//! be shared by crates (e.g. shogun-core's log path) without pulling in rusqlite/sqlcipher.
pub use shogun_redact::{redact, redact_log};
