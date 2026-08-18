//! OCR gate — architecture mirrored from Screenpipe `screenpipe-capture/src/ocr_gate.rs`
//! (issue #106 decision B). Screenpipe Commercial License prevents vendoring; logic is
//! reimplemented to match their pixel-signature skip semantics (#5060).
//! Reference: https://github.com/screenpipe/screenpipe

use std::collections::{HashMap, VecDeque};

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

/// Hard cap on tracked apps. Each entry caches one window's flat OCR text, so an unbounded map
/// is unbounded RAM; 64 apps is far beyond a real focus rotation.
const MAX_APPS: usize = 64;

/// Per-app gate keyed by lowercased bundle id or app name (Screenpipe keys by app name).
///
/// The caller (`capture_source::focus_key`) passes `"{bundle_id}\0{window_title}"`; the title
/// half is dropped here (see [`gate_key`]) so the gate is really per-app as documented —
/// otherwise every tab/document title would mint its own permanent entry holding full OCR text.
/// The map is additionally LRU-capped at [`MAX_APPS`] so it can never grow without bound.
#[derive(Debug, Default)]
pub struct OcrGate {
    apps: HashMap<String, AppGate>,
    /// Recency order for LRU eviction — least-recent at the front, most-recent at the back.
    recency: VecDeque<String>,
}

/// The gate key: the bundle-id prefix of the caller's `"{bundle}\0{title}"` focus key.
/// A key without the `'\0'` separator is used as-is.
fn gate_key(app_key: &str) -> &str {
    app_key.split('\0').next().unwrap_or(app_key)
}

impl OcrGate {
    /// Mark `key` most-recent; evict the least-recent entries beyond [`MAX_APPS`].
    fn touch(&mut self, key: &str) {
        if let Some(pos) = self.recency.iter().position(|k| k == key) {
            self.recency.remove(pos);
        }
        self.recency.push_back(key.to_string());
        while self.recency.len() > MAX_APPS {
            if let Some(evicted) = self.recency.pop_front() {
                self.apps.remove(&evicted);
            }
        }
    }

    /// `crop_signature` is [`crate::visual_recall::text_regions::image_pixel_signature`] of the
    /// padded union crop. Callers with no detected text regions skip without calling this.
    pub fn observe(&mut self, app_key: &str, crop_signature: u64) -> OcrDecision {
        let key = gate_key(app_key).to_string();
        self.touch(&key);
        let gate = self.apps.entry(key).or_default();
        if gate.last_ocr_signature == Some(crop_signature) {
            return OcrDecision::Skip;
        }
        gate.pending_ocr_signature = Some(crop_signature);
        OcrDecision::Ocr
    }

    /// Cached flat OCR text for a [`Skip`] tick (terminals / OCR-only surfaces).
    pub fn indexed_text(&self, app_key: &str) -> Option<&str> {
        self.apps.get(gate_key(app_key))?.indexed_text.as_deref()
    }

    /// Commit after durable persistence. Empty OCR still commits so identical frames skip.
    pub fn ocr_indexed(&mut self, app_key: &str, text: &str) {
        if let Some(gate) = self.apps.get_mut(gate_key(app_key)) {
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
        let mut gate = OcrGate::default();
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Ocr);
        gate.ocr_indexed("zoom", "hello world");
        for _ in 0..5 {
            assert_eq!(gate.observe("zoom", 1), OcrDecision::Skip);
        }
    }

    #[test]
    fn unpersisted_ocr_retries_until_committed() {
        let mut gate = OcrGate::default();
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Ocr);
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Ocr);
        gate.ocr_indexed("zoom", "hello world");
        assert_eq!(gate.observe("zoom", 1), OcrDecision::Skip);
    }

    #[test]
    fn title_suffix_shares_one_bundle_entry() {
        // The caller keys by "{bundle}\0{title}" — every title must land in the same app gate.
        let mut gate = OcrGate::default();
        assert_eq!(gate.observe("com.app\0Tab A", 1), OcrDecision::Ocr);
        gate.ocr_indexed("com.app\0Tab A", "hello");
        assert_eq!(gate.observe("com.app\0Tab B", 1), OcrDecision::Skip);
        assert_eq!(gate.indexed_text("com.app\0Tab B"), Some("hello"));
        assert_eq!(gate.apps.len(), 1);
    }

    #[test]
    fn map_is_capped_and_evicts_least_recent() {
        let mut gate = OcrGate::default();
        for i in 0..(MAX_APPS + 5) {
            gate.observe(&format!("app-{i}"), 1);
        }
        assert_eq!(gate.apps.len(), MAX_APPS);
        assert_eq!(gate.recency.len(), MAX_APPS);
        // The oldest entries are gone; the newest survives.
        assert!(!gate.apps.contains_key("app-0"));
        assert!(gate.apps.contains_key(&format!("app-{}", MAX_APPS + 4)));
    }
}
