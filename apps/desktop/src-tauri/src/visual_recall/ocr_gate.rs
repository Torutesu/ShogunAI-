//! OCR gate — architecture mirrored from Screenpipe `screenpipe-capture/src/ocr_gate.rs`
//! (issue #106 decision B). Screenpipe Commercial License prevents vendoring; logic is
//! reimplemented to match their pixel-signature skip semantics (#5060).
//! Reference: https://github.com/screenpipe/screenpipe

use std::collections::HashMap;

/// What OCR should do for the current gated capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrDecision {
    /// Crop is pixel-identical to the last indexed OCR — skip the Vision pass.
    Skip,
    /// Crop changed — run OCR on the union region.
    Ocr,
}

#[derive(Debug, Default)]
struct AppGate {
    pending_ocr_signature: Option<u64>,
    last_ocr_signature: Option<u64>,
    indexed_text: Option<String>,
}

/// Per-app gate keyed by lowercased bundle id or app name (Screenpipe keys by app name).
#[derive(Debug, Default)]
pub struct OcrGate {
    apps: HashMap<String, AppGate>,
}

impl OcrGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.apps.clear();
    }

    /// `crop_signature` is [`crate::visual_recall::text_regions::image_pixel_signature`] of the
    /// padded union crop. Callers with no detected text regions skip without calling this.
    pub fn observe(&mut self, app_key: &str, crop_signature: u64) -> OcrDecision {
        let gate = self.apps.entry(app_key.to_string()).or_default();
        if gate.last_ocr_signature == Some(crop_signature) {
            return OcrDecision::Skip;
        }
        gate.pending_ocr_signature = Some(crop_signature);
        OcrDecision::Ocr
    }

    /// Cached flat OCR text for a [`Skip`] tick (terminals / OCR-only surfaces).
    pub fn indexed_text(&self, app_key: &str) -> Option<&str> {
        self.apps.get(app_key)?.indexed_text.as_deref()
    }

    /// Commit after durable persistence. Empty OCR still commits so identical frames skip.
    pub fn ocr_indexed(&mut self, app_key: &str, text: &str) {
        if let Some(gate) = self.apps.get_mut(app_key) {
            if let Some(sig) = gate.pending_ocr_signature.take() {
                gate.last_ocr_signature = Some(sig);
                gate.indexed_text = Some(text.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sighting_ocrs_then_identical_crop_skips() {
        let mut gate = OcrGate::new();
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Ocr);
        gate.ocr_indexed("zoom", "hello world");
        for _ in 0..5 {
            assert_eq!(gate.observe("zoom", 1), OcrDecision::Skip);
        }
    }

    #[test]
    fn unpersisted_ocr_retries_until_committed() {
        let mut gate = OcrGate::new();
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Ocr);
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Ocr);
        gate.ocr_indexed("zoom", "hello world");
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Skip);
    }
}
