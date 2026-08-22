//! Secret locker. Never a file.
//!
//! Windows: Credential Manager (`CRED_TYPE_GENERIC`, persist-local). Linux: fail closed until
//! Secret Service is wired — writing `~/.config/shogunai/*.key` would violate invariant 7.

use serde::Serialize;

/// Named slots SHOGUN is allowed to store. Free-form target strings from the webview are rejected
/// so a renderer bug cannot write into another app's credential or a vendor CLI's slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSlot {
    /// Licence key (`shogun-XXXX-…`). The signed licence *token* is not a secret and does not live here.
    LicenseKey,
    /// Agent-lane BYOK. Subscription delegation does not use this slot.
    AgentKey,
}

impl SecretSlot {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LicenseKey => "license-key",
            Self::AgentKey => "agent-key",
        }
    }

    /// Credential Manager target. `ShogunAI:` prefix namespaces us away from other Generic creds.
    pub const fn target_name(self) -> &'static str {
        match self {
            Self::LicenseKey => "ShogunAI:license-key",
            Self::AgentKey => "ShogunAI:agent-key",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretStoreStatus {
    /// Pre-formatted for the Settings pane. OS names, not our stack.
    pub backend: &'static str,
    pub ready: bool,
    pub detail: &'static str,
}

#[derive(Debug)]
pub enum SecretError {
    Unsupported(&'static str),
    /// OS error text. Must never include the secret payload.
    Backend(&'static str),
    Empty,
    TooLarge,
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(msg) => f.write_str(msg),
            Self::Backend(msg) => f.write_str(msg),
            Self::Empty => f.write_str("refusing to store an empty secret"),
            Self::TooLarge => f.write_str("secret exceeds the OS locker limit"),
        }
    }
}

impl std::error::Error for SecretError {}

/// Credential blob cap well under the OS 5 KiB ceiling so we fail before CredWrite does.
const MAX_SECRET_BYTES: usize = 2048;

pub fn secret_store_status() -> SecretStoreStatus {
    #[cfg(windows)]
    {
        SecretStoreStatus {
            backend: "Windows Credential Manager",
            ready: true,
            detail: "Keys stay in the OS locker. SHOGUN does not write them to a file.",
        }
    }
    #[cfg(target_os = "linux")]
    {
        SecretStoreStatus {
            backend: "Linux secret store",
            ready: false,
            detail: "Not wired in this slice. Secrets are not stored on disk.",
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        SecretStoreStatus {
            backend: "Unavailable",
            ready: false,
            detail: "This shell targets Windows and Linux.",
        }
    }
}

pub fn set_secret(slot: SecretSlot, value: &str) -> Result<(), SecretError> {
    if value.is_empty() {
        return Err(SecretError::Empty);
    }
    if value.len() > MAX_SECRET_BYTES {
        return Err(SecretError::TooLarge);
    }
    set_secret_os(slot.target_name(), value)
}

pub fn get_secret(slot: SecretSlot) -> Result<Option<String>, SecretError> {
    get_secret_os(slot.target_name())
}

pub fn delete_secret(slot: SecretSlot) -> Result<(), SecretError> {
    delete_secret_os(slot.target_name())
}

#[cfg(windows)]
fn set_secret_os(target: &str, value: &str) -> Result<(), SecretError> {
    windows_cred::write(target, value)
}

#[cfg(windows)]
fn get_secret_os(target: &str) -> Result<Option<String>, SecretError> {
    windows_cred::read(target)
}

#[cfg(windows)]
fn delete_secret_os(target: &str) -> Result<(), SecretError> {
    windows_cred::delete(target)
}

#[cfg(not(windows))]
fn set_secret_os(_target: &str, _value: &str) -> Result<(), SecretError> {
    Err(SecretError::Unsupported(
        "Linux Secret Service is not wired in this slice. Secrets are not stored on disk.",
    ))
}

#[cfg(not(windows))]
fn get_secret_os(_target: &str) -> Result<Option<String>, SecretError> {
    Ok(None)
}

#[cfg(not(windows))]
fn delete_secret_os(_target: &str) -> Result<(), SecretError> {
    Ok(())
}

#[cfg(windows)]
mod windows_cred {
    use super::{SecretError, MAX_SECRET_BYTES};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, FALSE, FILETIME, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn write(target: &str, value: &str) -> Result<(), SecretError> {
        if value.len() > MAX_SECRET_BYTES {
            return Err(SecretError::TooLarge);
        }
        let mut target_w = wide(target);
        let mut user_w = wide("ShogunAI");
        let mut blob = value.as_bytes().to_vec();
        let cred = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target_w.as_mut_ptr(),
            Comment: std::ptr::null_mut(),
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: std::ptr::null_mut(),
            UserName: user_w.as_mut_ptr(),
        };
        // SAFETY: `cred` pointers address live `Vec` buffers that outlive this call. CredWriteW
        // copies the blob; we do not keep the OS pointer.
        let ok = unsafe { CredWriteW(&cred, 0) };
        if ok == FALSE {
            return Err(SecretError::Backend("Credential Manager refused the write"));
        }
        Ok(())
    }

    pub fn read(target: &str) -> Result<Option<String>, SecretError> {
        let target_w = wide(target);
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target_w` is a NUL-terminated wide string. On success `cred` is an OS allocation
        // we free via CredFree below.
        let ok = unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) };
        if ok == FALSE {
            let err = unsafe { GetLastError() };
            if err == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(SecretError::Backend("Credential Manager refused the read"));
        }
        struct Guard(*mut CREDENTIALW);
        impl Drop for Guard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    // SAFETY: pointer from CredReadW; exclusive to this guard.
                    unsafe { CredFree(self.0.cast()) };
                }
            }
        }
        let _guard = Guard(cred);
        if cred.is_null() {
            return Ok(None);
        }
        // SAFETY: CredReadW returned TRUE and a non-null credential.
        let rec = unsafe { &*cred };
        let n = rec.CredentialBlobSize as usize;
        if n == 0 || rec.CredentialBlob.is_null() {
            return Ok(None);
        }
        let bytes = unsafe { std::slice::from_raw_parts(rec.CredentialBlob, n) };
        String::from_utf8(bytes.to_vec())
            .map(Some)
            .map_err(|_| SecretError::Backend("stored secret was not UTF-8"))
    }

    pub fn delete(target: &str) -> Result<(), SecretError> {
        let target_w = wide(target);
        let ok = unsafe { CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok == FALSE {
            let err = unsafe { GetLastError() };
            if err == ERROR_NOT_FOUND {
                return Ok(());
            }
            return Err(SecretError::Backend(
                "Credential Manager refused the delete",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_namespaced() {
        assert!(SecretSlot::LicenseKey.target_name().starts_with("ShogunAI:"));
        assert!(SecretSlot::AgentKey.target_name().starts_with("ShogunAI:"));
        assert_ne!(
            SecretSlot::LicenseKey.target_name(),
            SecretSlot::AgentKey.target_name()
        );
    }

    #[test]
    fn empty_secret_is_rejected() {
        let err = set_secret(SecretSlot::AgentKey, "").unwrap_err();
        assert!(matches!(err, SecretError::Empty));
    }

    #[test]
    fn oversized_secret_is_rejected() {
        let big = "a".repeat(MAX_SECRET_BYTES + 1);
        let err = set_secret(SecretSlot::AgentKey, &big).unwrap_err();
        assert!(matches!(err, SecretError::TooLarge));
    }

    #[test]
    fn status_never_looks_like_a_file_store() {
        let s = secret_store_status();
        let blob = format!("{} {} {}", s.backend, s.detail, if s.ready { "ready" } else { "not" });
        assert!(!blob.to_lowercase().contains(".env"));
        assert!(!blob.to_lowercase().contains("plaintext"));
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_set_fails_closed() {
        let err = set_secret(SecretSlot::AgentKey, "not-a-real-key").unwrap_err();
        assert!(matches!(err, SecretError::Unsupported(_)));
        assert!(get_secret(SecretSlot::AgentKey).unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn credential_manager_roundtrip() {
        // Isolated target so a failed test cannot clobber a real AgentKey the developer stored.
        let target = "ShogunAI:test-roundtrip";
        let value = "test-value-not-a-key";
        windows_cred::write(target, value).expect("CredWrite");
        let read = windows_cred::read(target).expect("CredRead");
        windows_cred::delete(target).expect("CredDelete");
        assert_eq!(read.as_deref(), Some(value));
        assert!(windows_cred::read(target).expect("CredRead after delete").is_none());
    }
}
