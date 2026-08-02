//! macOS Keychain helpers with stable dev access (invariant 7).
//!
//! Default `SecItemAdd` ACL binds to the creating binary's cdhash, so each `cargo build` /
//! `tauri dev` rebuild invalidates "Always Allow" and macOS re-prompts. We store secrets with an
//! open ACL (`SecAccessCreate(NULL, …)`) and cache reads in-process so one unlock covers the
//! whole session. Legacy entries under service `SHOGUN` are read once and migrated to
//! [`SERVICE`] on repair.

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
use security_framework_sys::keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete};

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

static CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecAttrAccess: core_foundation_sys::string::CFStringRef;
    static kSecAttrAccessible: core_foundation_sys::string::CFStringRef;
    static kSecAttrAccessibleAfterFirstUnlock: core_foundation_sys::string::CFStringRef;

    fn SecAccessCreate(
        trusted_list: core_foundation_sys::array::CFArrayRef,
        trusted_list_for_confirm: core_foundation_sys::array::CFArrayRef,
        owner_is_trusted: u8,
        access: *mut security_framework_sys::base::SecAccessRef,
    ) -> i32;
}

/// Read a generic-password secret. Tries [`SERVICE`], then [`LEGACY_SERVICE`]. Cached after first
/// successful read for this process.
pub fn get_generic_secret(account: &str) -> Result<Vec<u8>> {
    if let Ok(guard) = cache().lock() {
        if let Some(bytes) = guard.get(account) {
            return Ok(bytes.clone());
        }
    }
    let bytes = read_from_keychain(SERVICE, account)
        .or_else(|_| read_from_keychain(LEGACY_SERVICE, account))?;
    if let Ok(mut guard) = cache().lock() {
        guard.insert(account.to_string(), bytes.clone());
    }
    Ok(bytes)
}

/// Store a generic-password secret under [`SERVICE`] with an open ACL (any app, no cdhash bind).
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
    delete_from_keychain(SERVICE, account)?;
    if let Ok(mut guard) = cache().lock() {
        guard.remove(account);
    }
    Ok(())
}

/// Re-save known accounts with the open ACL so future debug rebuilds stop prompting.
///
/// Call once near startup after the user has authorized access. Each account that exists is read
/// (possibly from cache) and written back with the stable ACL.
pub fn repair_dev_acls(accounts: &[&str]) {
    for account in accounts {
        match get_generic_secret(account) {
            Ok(bytes) => {
                if let Err(e) = set_generic_secret(account, &bytes) {
                    eprintln!("[keychain] could not repair ACL for {account}: {e}");
                }
            }
            Err(_) => {}
        }
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
    let _ = delete_from_keychain(service, account);
    let mut attrs = base_attrs(service, account);
    let access = open_access()?;
    let accessible = unsafe { CFString::wrap_under_get_rule(kSecAttrAccessibleAfterFirstUnlock) };
    attrs.push((
        unsafe { CFString::wrap_under_get_rule(kSecAttrAccess) },
        access,
    ));
    attrs.push((
        unsafe { CFString::wrap_under_get_rule(kSecAttrAccessible) },
        accessible.into_CFType(),
    ));
    attrs.push((
        unsafe { CFString::wrap_under_get_rule(kSecValueData) },
        CFData::from_buffer(password).into_CFType(),
    ));
    let dict = CFDictionary::from_CFType_pairs(&attrs);
    let mut ret = std::ptr::null();
    cvt(unsafe { SecItemAdd(dict.as_concrete_TypeRef(), &mut ret) })
}

fn delete_from_keychain(service: &str, account: &str) -> Result<()> {
    let query = query_dict(service, account, false);
    cvt(unsafe { SecItemDelete(query.as_concrete_TypeRef()) })
}

fn open_access() -> Result<core_foundation::base::CFType> {
    let mut access = std::ptr::null_mut();
    // NULL trusted list → any application may access without a per-cdhash prompt.
    cvt(unsafe { SecAccessCreate(std::ptr::null(), std::ptr::null(), 1, &mut access) })?;
    Ok(unsafe { core_foundation::base::CFType::wrap_under_create_rule(access as CFTypeRef) })
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
