//! Fetch a pinned **static model asset** over the traced egress (feature `net`).
//!
//! FR-TR-03 requires that the single raw HTTP client lives in shogun-core, the one traced egress —
//! no other crate may reach for reqwest directly. The whisper turbo weights the desktop fetches on
//! first use are a static asset (not user data), but the *download itself* is still an HTTP egress,
//! so it belongs here rather than in the desktop shell.
//!
//! **Invariant 2 does not apply:** this is a static model asset, not user audio. Writing it to disk
//! is fine. What we must not do is hold the whole file in RAM — so the stream is copied straight to
//! the temp file, hashed on the way through, verified, and only then atomically moved into place.
//!
//! **Degradation is the rule.** Any failure (no network, a truncated download, a hash mismatch)
//! returns an `Err`; the caller deletes the temp file and falls back to its bundled default. Nothing
//! here panics.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Stream `url` into `tmp`, verify its SHA256 against `expected_sha256` (lowercase hex; `None` skips
/// verification with a loud warning), then atomically rename `tmp` → `dest`. Returns the number of
/// bytes written on success. On any error the caller is responsible for removing `tmp`.
///
/// `tmp` and `dest` must live on the same filesystem (put `tmp` in the same directory as `dest`) so
/// the final rename is atomic and a crash mid-download never leaves a half-written asset at `dest`.
pub fn download_verified(
    url: &str,
    tmp: &Path,
    dest: &Path,
    expected_sha256: Option<&str>,
) -> Result<u64, String> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("unexpected status {}", resp.status()));
    }

    // Stream response → temp file while hashing on the way through, so the file is never held in
    // RAM in full and we do not re-read it from disk to hash it.
    let mut file = std::fs::File::create(tmp).map_err(|e| format!("create temp: {e}"))?;
    let mut hasher = Sha256::new();
    let mut reader = resp;
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        use std::io::Write;
        file.write_all(&buf[..n]).map_err(|e| format!("write: {e}"))?;
        total += n as u64;
    }
    file.sync_all().map_err(|e| format!("sync: {e}"))?;
    drop(file);

    let digest = hasher.finalize();
    let got = hex_lower(&digest);
    match expected_sha256 {
        Some(expected) => {
            if !got.eq_ignore_ascii_case(expected) {
                return Err(format!("sha256 mismatch: got {got}"));
            }
        }
        None => {
            crate::elog!("[model_asset] WARNING: SHA256 not pinned; skipping verification (got {got})");
        }
    }

    std::fs::rename(tmp, dest).map_err(|e| format!("rename into place: {e}"))?;
    Ok(total)
}

/// Lowercase hex of a byte slice. Small and dependency-free (avoids pulling in `hex`).
fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_lower_encodes_bytes() {
        assert_eq!(hex_lower(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }
}
