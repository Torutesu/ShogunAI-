//! Visual recall settings (issue #107).
//!
//! When enabled, the macOS adapter may capture a focused window into RAM, run on-device OCR
//! (Apple Vision), persist text + provenance, and optionally retain compressed JPEG frames
//! for a user-selected rolling window (`screen_frames` — explicit invariant-2 exception).
//! Default is **off**.

use std::path::Path;

pub const DEFAULT_RETENTION_DAYS: u8 = 3;
pub const RETENTION_DAY_OPTIONS: [u8; 4] = [1, 3, 5, 7];
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn default_retention_days() -> u8 {
    DEFAULT_RETENTION_DAYS
}

pub fn valid_retention_days(days: u8) -> bool {
    RETENTION_DAY_OPTIONS.contains(&days)
}

/// Persisted visual-recall preference. Default off; serde defaults keep partial files safe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// Master switch. Off means no screen pixels are read and no OCR runs.
    #[serde(default)]
    pub enabled: bool,
    /// Rolling lifetime for saved Visual Recall frames.
    #[serde(default = "default_retention_days")]
    pub retention_days: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

impl Settings {
    pub fn retention_ms(&self) -> i64 {
        i64::from(self.retention_days) * DAY_MS
    }

    fn sanitized(mut self) -> Self {
        if !valid_retention_days(self.retention_days) {
            self.retention_days = DEFAULT_RETENTION_DAYS;
        }
        self
    }
}

/// Load settings from `path`, or default when missing/unreadable.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default()
        .sanitized()
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
        assert_eq!(Settings::default().retention_days, 3);
    }

    #[test]
    fn absent_json_field_reads_as_off() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.enabled);
        assert_eq!(s.retention_days, 3);
    }

    #[test]
    fn invalid_retention_on_disk_falls_back_to_three_days() {
        let settings: Settings =
            serde_json::from_str(r#"{"enabled":true,"retention_days":30}"#).unwrap();
        let settings = settings.sanitized();
        assert!(settings.enabled);
        assert_eq!(settings.retention_days, DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn every_retention_preset_has_expected_milliseconds() {
        for days in RETENTION_DAY_OPTIONS {
            let settings = Settings {
                enabled: false,
                retention_days: days,
            };
            assert_eq!(settings.retention_ms(), i64::from(days) * DAY_MS);
        }
    }
}
