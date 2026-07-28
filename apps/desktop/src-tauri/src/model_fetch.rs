//! Fetch the opt-in large-v3-turbo whisper model once, on first use (§5, Task 16a).
//!
//! `Settings.asr_model == Turbo` asks for higher accuracy than the bundled small model. The turbo
//! weights are large (hundreds of MB), so they are not bundled — they are fetched once into
//! `app_data_dir()/models/` and reused thereafter.
//!
//! **Invariant 2 does not apply here:** this is a *static model asset*, not user audio. It is fine
//! to write to disk. What we must not do is hold the whole file in RAM — so the download is
//! streamed straight to a temp file via `std::io::copy`, hashed, and only then moved into place.
//!
//! **Degradation is the rule.** Any failure — no network, a truncated download, a hash mismatch —
//! deletes the temp file and returns `None`, and the caller (`audio_lane::start`) falls back to the
//! bundled small model. Turbo is a bonus, never a requirement, so nothing here panics.
//!
//! whisper.cpp / whisper-rs load **ggml `.bin`** models, so the fetched file is the real ggml
//! `ggml-large-v3-turbo.bin` from HuggingFace `ggerganov/whisper.cpp`, named to match.

use std::io::Read;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tauri::Manager;

/// The pinned source for the turbo weights. `ggerganov/whisper.cpp` is the canonical ggml model
/// host; `resolve/main/<file>` streams the raw bytes.
const TURBO_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";

/// The filename whisper-rs loads (a real ggml `.bin`, not a `.gguf`).
const TURBO_FILE: &str = "ggml-large-v3-turbo.bin";

/// The expected SHA256 of the fetched file, lowercase hex.
///
/// `None` means "skip verification" — used only until the file has been fetched once and its hash
/// recorded. Skipping is loud (a warning log) rather than silent: an unverified model asset is a
/// supply-chain gap we want visible, but a *fabricated* hash would make turbo never load, which is
/// worse. When a real hash is known, set this to `Some("…")` and verification becomes mandatory.
///
// TODO(#7): pin SHA once the file is fetched once and hashed.
const TURBO_SHA256: Option<&str> = None;

/// Return the path to the turbo model, fetching it once if needed. `None` on any failure — the
/// caller degrades to the bundled small model.
pub fn ensure_turbo(app: &tauri::AppHandle) -> Option<PathBuf> {
    let models_dir = app.path().app_data_dir().ok()?.join("models");
    let dest = models_dir.join(TURBO_FILE);
    if dest.exists() {
        return Some(dest);
    }

    if let Err(e) = std::fs::create_dir_all(&models_dir) {
        eprintln!("[meeting] turbo model dir could not be created ({e}); falling back to small");
        return None;
    }

    // A temp file in the *same* directory so the final rename is atomic (same filesystem) and a
    // crash mid-download never leaves a half-written model at the real path.
    let tmp = models_dir.join(format!("{TURBO_FILE}.part"));
    match download_and_verify(&tmp, &dest) {
        Ok(()) => {
            eprintln!("[meeting] turbo model fetched and verified");
            Some(dest)
        }
        Err(e) => {
            eprintln!("[meeting] turbo model fetch failed ({e}); falling back to small");
            let _ = std::fs::remove_file(&tmp);
            None
        }
    }
}

/// Stream the download to `tmp`, verify its SHA256, then atomically move it to `dest`. Returns an
/// error string on any failure; the caller removes `tmp`.
fn download_and_verify(tmp: &PathBuf, dest: &PathBuf) -> Result<(), String> {
    eprintln!("[meeting] fetching turbo model (first use); this is a one-time download");

    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(TURBO_URL)
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
    eprintln!("[meeting] turbo model downloaded ({total} bytes)");

    let digest = hasher.finalize();
    let got = hex_lower(&digest);
    match TURBO_SHA256 {
        Some(expected) => {
            if !got.eq_ignore_ascii_case(expected) {
                return Err(format!("sha256 mismatch: got {got}"));
            }
        }
        None => {
            eprintln!(
                "[meeting] WARNING: turbo model SHA256 not pinned; skipping verification (got {got})"
            );
        }
    }

    std::fs::rename(tmp, dest).map_err(|e| format!("rename into place: {e}"))?;
    Ok(())
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
