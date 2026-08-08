//! Visual recall settings (issue #107).
//!
//! When enabled, the macOS adapter may capture a focused window into RAM, run on-device OCR
//! (Apple Vision), persist text + provenance, and optionally retain compressed JPEG frames
//! for ≤72 h (`screen_frames` — explicit invariant-2 exception, user decision 2026-08-02).
//! Default is **off**.

use std::path::Path;

/// Persisted visual-recall preference. Default off; serde defaults keep partial files safe.
#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// Master switch. Off means no screen pixels are read and no OCR runs.
    #[serde(default)]
    pub enabled: bool,
}

/// Load settings from `path`, or default when missing/unreadable.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Persist settings to `path` (creates parent dirs when possible).
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create visual_recall settings directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("visual_recall.json");
    let temp_path = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&temp_path, json)
        .map_err(|e| format!("save visual_recall settings temp file: {e}"))?;
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!("replace visual_recall settings: {e}")
    })
}

/// Re-OCR a stored JPEG via Apple Vision (macOS). Returns `None` when OCR fails or unavailable.
#[cfg(target_os = "macos")]
pub fn ocr_jpeg_bytes(jpeg: &[u8]) -> Option<String> {
    use objc2::AnyThread;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizedTextObservation, VNRecognizeTextRequest, VNRequest,
        VNRequestTextRecognitionLevel,
    };

    let data = NSData::with_bytes(jpeg);
    let options = NSDictionary::<objc2_vision::VNImageOption, AnyObject>::new();
    let handler = unsafe {
        VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &options)
    };
    let request = unsafe {
        let req = VNRecognizeTextRequest::init(VNRecognizeTextRequest::alloc());
        req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        req.setUsesLanguageCorrection(true);
        req.setAutomaticallyDetectsLanguage(true);
        req
    };
    let requests = NSArray::from_slice(&[&*request as &VNRequest]);
    handler.performRequests_error(&requests).ok()?;
    let observations = request.results()?;
    let mut lines: Vec<String> = Vec::new();
    let mut conf_sum = 0.0f32;
    for obs in observations.iter() {
        let Some(text_obs) = obs.downcast_ref::<VNRecognizedTextObservation>() else {
            continue;
        };
        let candidates = text_obs.topCandidates(1);
        if let Some(best) = candidates.firstObject() {
            let line = best.string().to_string();
            if line.trim().is_empty() {
                continue;
            }
            conf_sum += best.confidence();
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return None;
    }
    let n = lines.len() as f32;
    eprintln!(
        "[visual_recall] re-ocr lines={} mean_conf={:.2} chars={}",
        lines.len(),
        conf_sum / n,
        lines.iter().map(|l| l.len()).sum::<usize>() + lines.len().saturating_sub(1)
    );
    Some(lines.join("\n"))
}

#[cfg(not(target_os = "macos"))]
pub fn ocr_jpeg_bytes(_jpeg: &[u8]) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off() {
        assert!(!Settings::default().enabled);
    }

    #[test]
    fn absent_json_field_reads_as_off() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.enabled);
    }
}
