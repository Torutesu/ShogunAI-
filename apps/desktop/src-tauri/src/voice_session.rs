//! Voice hold-to-talk session entrypoint (#44).
//!
//! Focused modules own lifecycle, consent, ASR, dictionary correction, optional editing, and
//! guarded Accessibility insertion. Public command and shortcut paths remain `voice_session::mac`.

#[cfg(target_os = "macos")]
pub mod mac;
