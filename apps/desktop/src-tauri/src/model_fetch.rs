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

use std::path::PathBuf;

use tauri::Manager;

/// The pinned source for the turbo weights. `ggerganov/whisper.cpp` is the canonical ggml model
/// host; `resolve/main/<file>` streams the raw bytes.
const TURBO_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";

/// The filename whisper-rs loads (a real ggml `.bin`, not a `.gguf`).
const TURBO_FILE: &str = "ggml-large-v3-turbo.bin";

/// The expected SHA256 of the fetched file, lowercase hex.
///
/// `None` would mean "skip verification" (loud warning). Now pinned: the SHA-256 of
/// `ggml-large-v3-turbo.bin` as fetched from `ggerganov/whisper.cpp` on 2026-07-28. A mismatch
/// makes `ensure_turbo` reject the download and fall back to the bundled small model, so a
/// corrupted or swapped asset can never be loaded.
const TURBO_SHA256: Option<&str> =
    Some("1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69");

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
    eprintln!("[meeting] fetching turbo model (first use); this is a one-time download");
    // FR-TR-03: the raw HTTP client lives in shogun-core (the one traced egress). The shell only
    // resolves the on-device path and pins the URL/hash — the download itself goes through core.
    match shogun_core::model_asset::download_verified(TURBO_URL, &tmp, &dest, TURBO_SHA256) {
        Ok(bytes) => {
            eprintln!("[meeting] turbo model fetched and verified ({bytes} bytes)");
            Some(dest)
        }
        Err(e) => {
            eprintln!("[meeting] turbo model fetch failed ({e}); falling back to small");
            let _ = std::fs::remove_file(&tmp);
            None
        }
    }
}

