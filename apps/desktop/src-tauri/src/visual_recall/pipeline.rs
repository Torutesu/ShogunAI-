//! Screenpipe-style OCR trigger + gated capture path (issue #106).
//!
//! Flow: CGWindow capture (RAM) → text-region detect → pixel-signature gate →
//! Apple Vision on union crop → text + provenance. On fresh Vision success the caller may
//! persist a compressed JPEG (72 h, see `screen_frames`) before dropping pixels.

use image::DynamicImage;
use image::GenericImageView;

use super::ocr_gate::{OcrDecision, OcrGate};
use super::text_regions::{detect_text_regions, image_pixel_signature, union_region, TextRegion};

/// Padding around the detected-text union crop (Screenpipe #5054 benchmark).
pub const UNION_PAD_PX: u32 = 20;

const CANVAS_APP_PATTERNS: &[&str] = &[
    "google docs",
    "google sheets",
    "google slides",
    "google drawings",
    "figma",
    "excalidraw",
    "miro",
    "canva",
    "tldraw",
];

/// Terminal emulators whose AX buffer is raw/unformatted — Screenpipe always OCRs these.
pub fn app_prefers_ocr(bundle_or_app: &str) -> bool {
    let n = bundle_or_app.to_lowercase();
    n.contains("wezterm")
        || n.contains("alacritty")
        || n.contains("kitty")
        || n.contains("hyper")
        || n.contains("warp")
}

/// AX tree returned text but likely missed canvas/GPU document body (Screenpipe thin heuristic).
pub fn a11y_content_is_thin(window_title: Option<&str>, ax_text_len: usize, meeting_active: bool) -> bool {
    if let Some(win) = window_title {
        let win_lower = win.to_lowercase();
        if CANVAS_APP_PATTERNS.iter().any(|pat| win_lower.contains(pat)) {
            return true;
        }
        if meeting_active
            && (win_lower.contains("presentation")
                || win_lower.contains("slide")
                || win_lower.contains("share")
                || win_lower.contains("screen share"))
        {
            return true;
        }
    }
    let thin_threshold = if meeting_active { 400 } else { 100 };
    ax_text_len < thin_threshold
}

/// Pre-gate OCR triggers from Screenpipe `paired_capture` (minus meeting-only gate).
pub fn wants_ocr(
    bundle_or_app: &str,
    window_title: Option<&str>,
    ax_empty: bool,
    ax_text_len: usize,
    meeting_active: bool,
) -> bool {
    let prefers = app_prefers_ocr(bundle_or_app);
    let has_ax = !prefers && !ax_empty;
    let thin = has_ax && a11y_content_is_thin(window_title, ax_text_len, meeting_active);
    prefers || !has_ax || thin
}

/// Outcome of one gated OCR attempt on the focused window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrOutcome {
    /// OCR not needed (AX sufficient, or gate skip with no cached text).
    Skipped,
    /// Vision ran but returned nothing usable.
    Empty,
    /// Extracted text ready for `ingest_screen_ocr`.
    Text(String),
}

/// Stateful Screenpipe-style OCR gate (one per capture poller).
#[derive(Debug, Default)]
pub struct RecallPipeline {
    gate: OcrGate,
}

impl RecallPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the gated OCR path on a focused-window image already in RAM.
    /// Returns `(outcome, fresh_vision)` — `fresh_vision` is true when Apple Vision ran this tick
    /// (not a pixel-signature cache hit).
    pub fn ocr_gated_window(
        &mut self,
        frame: &DynamicImage,
        app_key: &str,
        wants_ocr: bool,
        ocr_crop: impl FnOnce(&DynamicImage, TextRegion) -> Option<String>,
    ) -> (OcrOutcome, bool) {
        if !wants_ocr {
            return (OcrOutcome::Skipped, false);
        }

        let regions = detect_text_regions(frame);
        let (frame_w, frame_h) = frame.dimensions();
        let Some(union) = union_region(&regions, UNION_PAD_PX, frame_w, frame_h) else {
            return (OcrOutcome::Skipped, false);
        };
        let union_img = frame.crop_imm(union.x, union.y, union.width, union.height);
        let signature = image_pixel_signature(&union_img);

        match self.gate.observe(app_key, signature) {
            OcrDecision::Skip => {
                if let Some(cached) = self.gate.indexed_text(app_key) {
                    if cached.trim().is_empty() {
                        (OcrOutcome::Empty, false)
                    } else {
                        (OcrOutcome::Text(cached.to_string()), false)
                    }
                } else {
                    (OcrOutcome::Skipped, false)
                }
            }
            OcrDecision::Ocr => {
                let text = ocr_crop(frame, union).unwrap_or_default();
                if text.trim().is_empty() {
                    (OcrOutcome::Empty, true)
                } else {
                    self.gate.ocr_indexed(app_key, &text);
                    (OcrOutcome::Text(text), true)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_always_want_ocr() {
        assert!(wants_ocr("com.github.wez.wezterm", None, false, 5000, false));
    }

    #[test]
    fn rich_ax_skips_ocr() {
        assert!(!wants_ocr("com.apple.Safari", Some("Inbox"), false, 500, false));
    }

    #[test]
    fn thin_ax_triggers_ocr() {
        assert!(wants_ocr(
            "com.google.Chrome",
            Some("Q1 Plan - Google Docs"),
            false,
            200,
            false
        ));
    }

    #[test]
    fn meeting_relaxes_thin_threshold() {
        // The relaxation itself: same window, same AX text, meeting the only variable. 250 chars
        // clears the everyday bar of 100 but not the meeting bar of 400, so a meeting — and only a
        // meeting — makes this window thin enough to OCR (runbook §visual-recall: 本文 100 文字未満、
        // 会議中は 400 文字未満).
        assert!(!wants_ocr("com.apple.Safari", Some("Inbox"), false, 250, false));
        assert!(wants_ocr("com.apple.Safari", Some("Inbox"), false, 250, true));
        // The relaxed bar is still a bar — a meeting does not mean OCR everything.
        assert!(!wants_ocr("com.apple.Safari", Some("Inbox"), false, 500, true));
    }

    #[test]
    fn meeting_titles_trigger_ocr_over_a_full_ax_buffer() {
        // The title rule has to carry its own weight: at 500 chars the length rule says "not thin",
        // so if these pass it is because the title matched. Screen-shared slides are exactly the
        // case where a healthy AX buffer describes the app rather than the content on screen.
        assert!(wants_ocr("com.apple.Safari", Some("Slide deck"), false, 500, true));
        assert!(wants_ocr("com.apple.Safari", Some("Q3 presentation"), false, 500, true));
        // Outside a meeting the same titles are ordinary windows.
        assert!(!wants_ocr("com.apple.Safari", Some("Slide deck"), false, 500, false));
    }
}
