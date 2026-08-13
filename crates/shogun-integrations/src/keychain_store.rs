//! macOS Keychain helpers with stable dev access (invariant 7).
//!
//! Secrets use [`SERVICE`] with the default Keychain ACL. Custom `SecAccess` ACLs
//! (`SecAccessCreate`) return `OSStatus 4` on current macOS, so we store without `kSecAttrAccess`.
//! Reads are cached in-process so one unlock covers the whole session. Legacy entries under
//! service `SHOGUN` are read once and migrated to [`SERVICE`] on repair.

#![cfg(target_os = "macos")]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation_sys::base::CFTypeRef;
use security_framework::base::Result;
use security_framework_sys::base::errSecSuccess;
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrService, kSecClass, kSecClassGenericPassword, kSecReturnData,
    kSecValueData,
};
use security_framework_sys::keychain_item::{
    SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate,
};

fn cvt(status: i32) -> Result<()> {
    if status == errSecSuccess {
        Ok(())
    } else {
        Err(security_framework::base::Error::from_code(status))
    }
}

/// Keychain "service" field for every SHOGUN secret.
pub const SERVICE: &str = "com.selectkk.shogun";

/// Pre-unification service name still present on some dev machines.
const LEGACY_SERVICE: &str = "SHOGUN";

/// Keychain account for the Select KK credential (Batch lane + live meeting translate).
pub const SELECT_KK_ACCOUNT: &str = "select-kk-batch";

/// Keychain account for the user's Deepgram API key (meeting live STT).
pub const DEEPGRAM_ASR_ACCOUNT: &str = "deepgram-asr";
pub const GOOGLE_OAUTH_CLIENT_ID_ACCOUNT: &str = "google-oauth-client-id";
pub const GOOGLE_OAUTH_CLIENT_SECRET_ACCOUNT: &str = "google-oauth-client-secret";

static CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();

/// Read a generic-password secret. Tries [`SERVICE`], then [`LEGACY_SERVICE`]. Migrates legacy-only
/// entries to [`SERVICE`] on first read. Cached after first successful read for this process so
/// startup does not hit the Keychain once per subsystem (DB key, Select KK, Composio, …).
pub fn get_generic_secret(account: &str) -> Result<Vec<u8>> {
    if let Ok(guard) = cache().lock() {
        if let Some(bytes) = guard.get(account) {
            return Ok(bytes.clone());
        }
    }
    match read_from_keychain(SERVICE, account) {
        Ok(bytes) => {
            warm_cache(account, &bytes);
            Ok(bytes)
        }
        Err(_) => {
            let bytes = read_from_keychain(LEGACY_SERVICE, account)?;
            // Best-effort migration — do not fail the read if rewrite fails.
            if write_to_keychain(SERVICE, account, &bytes).is_ok() {
                let _ = delete_from_keychain(LEGACY_SERVICE, account);
            }
            warm_cache(account, &bytes);
            Ok(bytes)
        }
    }
}

/// Store a generic-password secret under [`SERVICE`].
pub fn set_generic_secret(account: &str, password: &[u8]) -> Result<()> {
    let _ = delete_from_keychain(LEGACY_SERVICE, account);
    write_to_keychain(SERVICE, account, password)?;
    if let Ok(mut guard) = cache().lock() {
        guard.insert(account.to_string(), password.to_vec());
    }
    Ok(())
}

/// Delete a secret from [`SERVICE`] and drop any in-process cache entry.
pub fn delete_generic_secret(account: &str) -> Result<()> {
    let _ = delete_from_keychain(LEGACY_SERVICE, account);
    match delete_from_keychain(SERVICE, account) {
        Ok(()) => {}
        Err(error) if error.code() == -25300 => {}
        Err(error) => return Err(error),
    }
    if let Ok(mut guard) = cache().lock() {
        guard.remove(account);
    }
    Ok(())
}

/// Whether a Select KK credential is present and looks like a real Anthropic API key.
pub fn select_kk_configured() -> bool {
    get_select_kk_key().is_some()
}

/// Read the Select KK API key. Keychain first (`SELECT_KK_ACCOUNT` under [`SERVICE`] or
/// [`LEGACY_SERVICE`]); `SHOGUN_SELECT_KK` env is dev provisioning only (recap_probe parity).
pub fn get_select_kk_key() -> Option<String> {
    #[cfg(debug_assertions)]
    if let Ok(k) = std::env::var("SHOGUN_SELECT_KK") {
        let trimmed = k.trim().to_string();
        if let Some(key) = normalize_select_kk_key(&trimmed) {
            return Some(key);
        }
    }
    let bytes = get_generic_secret(SELECT_KK_ACCOUNT).ok()?;
    let raw = decode_secret(bytes)?;
    let key = normalize_select_kk_key(&raw)?;
    // Dev machines sometimes store the key as hex (same shape as the DB encryption key). Heal once.
    if raw != key {
        if let Err(e) = set_generic_secret(SELECT_KK_ACCOUNT, key.as_bytes()) {
            eprintln!("[keychain] could not rewrite normalized {SELECT_KK_ACCOUNT}: {e}");
        } else {
            eprintln!("[keychain] normalized hex-encoded {SELECT_KK_ACCOUNT} to plain text");
        }
    }
    Some(key)
}

/// Whether a Deepgram API key is present in Keychain.
pub fn deepgram_asr_configured() -> bool {
    get_deepgram_asr_key().is_some()
}

/// Read the Deepgram API key from Keychain (`DEEPGRAM_ASR_ACCOUNT` under [`SERVICE`]).
pub fn get_deepgram_asr_key() -> Option<String> {
    let bytes = get_generic_secret(DEEPGRAM_ASR_ACCOUNT).ok()?;
    decode_secret(bytes)
}

/// Store the Deepgram API key for meeting live STT. Plain text only; never logged.
pub fn set_deepgram_asr_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("key is empty".into());
    }
    if key.chars().count() < 8 {
        return Err("key looks too short — paste the full Deepgram API key".into());
    }
    set_generic_secret(DEEPGRAM_ASR_ACCOUNT, key.as_bytes()).map_err(|e| e.to_string())
}

/// Store the Select KK API key (Dream Cycle, recap, live translation). Plain text only.
pub fn set_select_kk_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    let Some(normalized) = normalize_select_kk_key(key) else {
        return Err("key must start with sk-ant- (paste the API key, not hex)".into());
    };
    set_generic_secret(SELECT_KK_ACCOUNT, normalized.as_bytes()).map_err(|e| e.to_string())
}

/// Touch every known account once at startup so later subsystems reuse the in-process cache.
///
/// Missing accounts are ignored (`errSecItemNotFound` does not prompt). Existing items may still
/// show one Keychain dialog per launch in unsigned `tauri dev` builds — run
/// `scripts/codesign-desktop-dev.sh` after building so "Always Allow" survives rebuilds.
pub fn warm_startup_keychain(accounts: &[&str]) {
    for account in accounts {
        if let Ok(bytes) = get_generic_secret(account) {
            eprintln!("[keychain] warmed {account} ({} bytes)", bytes.len());
        }
    }
}

/// Migrate legacy `SHOGUN` service entries to [`SERVICE`] without delete-before-write.
#[deprecated(note = "use warm_startup_keychain — migration now happens inside get_generic_secret")]
pub fn repair_dev_acls(accounts: &[&str]) {
    warm_startup_keychain(accounts);
}

fn decode_secret(bytes: Vec<u8>) -> Option<String> {
    String::from_utf8(bytes)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Accept plain `sk-ant-…` keys and one common dev mistake: the key hex-encoded as ASCII.
fn normalize_select_kk_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("sk-ant-") {
        return Some(trimmed.to_string());
    }
    if trimmed.len() >= 40
        && trimmed.len() % 2 == 0
        && trimmed.chars().all(|c| c.is_ascii_hexdigit())
    {
        let decoded = decode_hex_ascii(trimmed)?;
        let text = String::from_utf8(decoded).ok()?;
        let text = text.trim();
        if text.starts_with("sk-ant-") {
            return Some(text.to_string());
        }
    }
    None
}

fn decode_hex_ascii(hex: &str) -> Option<Vec<u8>> {
    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_select_kk_key;

    #[test]
    fn select_kk_accepts_plain_key() {
        assert_eq!(
            normalize_select_kk_key("sk-ant-api03-abc"),
            Some("sk-ant-api03-abc".into())
        );
    }

    #[test]
    fn select_kk_decodes_hex_mistake() {
        // "sk-ant-api03-test-key-value" hex-encoded (devs sometimes store like the DB key)
        let hex = "736b2d616e742d61706930332d746573742d6b65792d76616c7565";
        assert_eq!(
            normalize_select_kk_key(hex),
            Some("sk-ant-api03-test-key-value".into())
        );
    }

    #[test]
    fn select_kk_rejects_garbage() {
        assert!(normalize_select_kk_key("not-a-key").is_none());
    }
}

fn read_from_keychain(service: &str, account: &str) -> Result<Vec<u8>> {
    let query = query_dict(service, account, true);
    let mut ret: CFTypeRef = std::ptr::null();
    cvt(unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut ret) })?;
    let data = unsafe { CFData::wrap_under_create_rule(ret as core_foundation_sys::data::CFDataRef) };
    Ok(data.bytes().to_vec())
}

fn write_to_keychain(service: &str, account: &str, password: &[u8]) -> Result<()> {
    let value = CFData::from_buffer(password);
    if read_from_keychain(service, account).is_ok() {
        let query = query_dict(service, account, false);
        let update = CFDictionary::from_CFType_pairs(&[(
            unsafe { CFString::wrap_under_get_rule(kSecValueData) },
            value.into_CFType(),
        )]);
        return cvt(unsafe {
            SecItemUpdate(query.as_concrete_TypeRef(), update.as_concrete_TypeRef())
        });
    }
    let mut attrs = base_attrs(service, account);
    attrs.push((
        unsafe { CFString::wrap_under_get_rule(kSecValueData) },
        value.into_CFType(),
    ));
    let dict = CFDictionary::from_CFType_pairs(&attrs);
    let mut ret = std::ptr::null();
    cvt(unsafe { SecItemAdd(dict.as_concrete_TypeRef(), &mut ret) })
}

fn delete_from_keychain(service: &str, account: &str) -> Result<()> {
    let query = query_dict(service, account, false);
    cvt(unsafe { SecItemDelete(query.as_concrete_TypeRef()) })
}

fn base_attrs(service: &str, account: &str) -> Vec<(CFString, core_foundation::base::CFType)> {
    vec![
        (
            unsafe { CFString::wrap_under_get_rule(kSecClass) },
            unsafe { CFString::wrap_under_get_rule(kSecClassGenericPassword) }.into_CFType(),
        ),
        (
            unsafe { CFString::wrap_under_get_rule(kSecAttrService) },
            CFString::new(service).into_CFType(),
        ),
        (
            unsafe { CFString::wrap_under_get_rule(kSecAttrAccount) },
            CFString::new(account).into_CFType(),
        ),
    ]
}

fn warm_cache(account: &str, bytes: &[u8]) {
    if let Ok(mut guard) = cache().lock() {
        guard.insert(account.to_string(), bytes.to_vec());
    }
}

fn cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn query_dict(service: &str, account: &str, return_data: bool) -> CFDictionary<CFString, core_foundation::base::CFType> {
    let mut attrs = base_attrs(service, account);
    if return_data {
        attrs.push((
            unsafe { CFString::wrap_under_get_rule(kSecReturnData) },
            CFBoolean::true_value().into_CFType(),
        ));
    }
    CFDictionary::from_CFType_pairs(&attrs)
}
