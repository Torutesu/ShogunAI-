//! Visual recall settings (issue #107).
//!
//! When enabled, the macOS adapter may capture a focused window into RAM, run on-device OCR
//! (Apple Vision), persist text + provenance, and optionally retain encrypted compressed JPEG
//! frames for the user-selected finite duration (`screen_frames` — invariant-2 exception).
//! Default is **off**.

use std::path::Path;

/// Existing installs and invalid retention values safely fall back to three days.
pub const DEFAULT_RETENTION_DAYS: u32 = 3;
/// Settings exposes one-day ticks through one week before the custom option.
pub const PRESET_RETENTION_DAYS: [u32; 7] = [1, 2, 3, 4, 5, 6, 7];
/// Custom retention is bounded to ten years. Automatic age expiry is always enabled.
pub const MAX_CUSTOM_RETENTION_DAYS: u32 = 3_650;
pub const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

/// Validated finite retention for encrypted Visual Recall JPEG rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RetentionPolicy {
    days: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            days: DEFAULT_RETENTION_DAYS,
        }
    }
}

impl RetentionPolicy {
    pub fn try_days(days: u32) -> Result<Self, String> {
        if !(1..=MAX_CUSTOM_RETENTION_DAYS).contains(&days) {
            return Err(format!(
                "retention days must be between 1 and {MAX_CUSTOM_RETENTION_DAYS}"
            ));
        }
        Ok(Self { days })
    }

    pub fn days(self) -> u32 {
        self.days
    }

    /// Convert the bounded day count without relying on unchecked duration arithmetic.
    pub fn retain_ms(self) -> Result<i64, String> {
        i64::from(self.days)
            .checked_mul(DAY_MS)
            .ok_or_else(|| "visual recall retention is too large".to_string())
    }

    pub fn from_ms(self, now_ms: i64) -> Result<i64, String> {
        self.retain_ms()
            .map(|retain_ms| now_ms.saturating_sub(retain_ms))
    }
}

/// Persisted visual-recall preference. Default off; serde defaults keep partial files safe.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Settings {
    /// Master switch. Off means no screen pixels are read and no OCR runs.
    pub enabled: bool,
    /// Automatic age expiry for JPEG rows. This can never represent an unbounded duration.
    pub retention: RetentionPolicy,
}

impl<'de> serde::Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RawRetention {
            days: u32,
        }

        #[derive(serde::Deserialize)]
        struct StoredSettings {
            #[serde(default)]
            enabled: bool,
            #[serde(default)]
            retention: Option<RawRetention>,
            // Source commit 83e7b1d stored this scalar. New writes omit it.
            #[serde(default)]
            retention_days: Option<u32>,
        }

        let stored = StoredSettings::deserialize(deserializer)?;
        let days = stored
            .retention
            .map(|retention| retention.days)
            .or(stored.retention_days)
            .unwrap_or(DEFAULT_RETENTION_DAYS);
        Ok(Self {
            enabled: stored.enabled,
            retention: RetentionPolicy::try_days(days).unwrap_or_default(),
        })
    }
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
    let handler =
        VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &options);
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
        let settings = Settings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.retention.days(), DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn absent_json_field_reads_as_off() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.enabled);
        assert_eq!(s.retention.days(), DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn legacy_retention_days_migrates_without_changing_enabled() {
        let settings: Settings =
            serde_json::from_str(r#"{"enabled":true,"retention_days":7}"#).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.retention.days(), 7);
    }

    #[test]
    fn invalid_retention_falls_back_to_three_days() {
        let settings: Settings =
            serde_json::from_str(r#"{"enabled":true,"retention":{"days":0}}"#).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.retention.days(), DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn custom_retention_boundaries_are_checked() {
        assert!(RetentionPolicy::try_days(1).is_ok());
        assert!(RetentionPolicy::try_days(MAX_CUSTOM_RETENTION_DAYS).is_ok());
        assert!(RetentionPolicy::try_days(0).is_err());
        assert!(RetentionPolicy::try_days(MAX_CUSTOM_RETENTION_DAYS + 1).is_err());
    }

    #[test]
    fn retention_conversion_is_exact_and_finite() {
        let retention = RetentionPolicy::try_days(7).unwrap();
        assert_eq!(retention.retain_ms(), Ok(7 * DAY_MS));
        assert_eq!(retention.from_ms(10 * DAY_MS), Ok(3 * DAY_MS));
    }
}
