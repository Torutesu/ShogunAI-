//! Visual recall settings + Tauri commands (issue #106/#107).
//!
//! Opt-in passive screen OCR (default off). Saved frames use a user-selected rolling retention.
//!
//! Memory API / MCP / CLI symmetry via shogun-mcp tools + DbBackend (invariant 6).

#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod ocr_gate;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod pipeline;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod text_regions;

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::{Mutex, RwLock};

    use shogun_core::capture::visual_recall::{valid_retention_days, Settings};
    use tauri::Manager;

    pub type SharedSettings = std::sync::Arc<RwLock<Settings>>;

    static LANE: Mutex<Option<SharedSettings>> = Mutex::new(None);
    static SETTINGS_PATH: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| crate::memory_data_dir(d).join("visual_recall.json"))
    }

    /// Reload settings from disk into RAM so Memory API writes are observed by the capture loop.
    pub fn refresh_settings(shared: &SharedSettings) -> Settings {
        let disk = SETTINGS_PATH
            .lock()
            .ok()
            .and_then(|g| {
                g.as_ref()
                    .map(|p| shogun_core::capture::visual_recall::load_settings(p))
            })
            .unwrap_or_default();
        if let Ok(mut live) = shared.write() {
            *live = disk.clone();
        }
        disk
    }

    pub fn init(app: &tauri::AppHandle) -> SharedSettings {
        let settings = settings_path(app)
            .as_deref()
            .map(shogun_core::capture::visual_recall::load_settings)
            .unwrap_or_default();
        let shared = std::sync::Arc::new(RwLock::new(settings.clone()));
        if let Ok(mut g) = LANE.lock() {
            *g = Some(shared.clone());
        }
        if let Ok(mut p) = SETTINGS_PATH.lock() {
            *p = settings_path(app);
        }
        eprintln!(
            "[visual_recall] screen OCR {}",
            if settings.enabled {
                "enabled"
            } else {
                "off (default)"
            }
        );
        shared
    }

    fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(p) = settings_path(app) else {
            return Ok(());
        };
        shogun_core::capture::visual_recall::save_settings(&p, settings)
    }

    #[tauri::command]
    pub fn get_visual_recall_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.read().ok().map(|v| v.clone())))
            .flatten()
            .unwrap_or_default()
    }

    #[tauri::command]
    pub fn set_visual_recall_enabled(
        enabled: bool,
        app: tauri::AppHandle,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Result<(), String> {
        let candidate = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(shared) = g.as_ref() else {
                return Err("not ready".into());
            };
            let current = shared.read().map_err(|_| "busy".to_string())?;
            Settings {
                enabled,
                ..current.clone()
            }
        };
        save(&app, &candidate)?;
        let Ok(g) = LANE.lock() else {
            return Err("busy".into());
        };
        let Some(shared) = g.as_ref() else {
            return Err("not ready".into());
        };
        let mut live = shared.write().map_err(|_| "busy".to_string())?;
        *live = candidate;
        if !enabled {
            // Passive OCR off: drop auto frames only; user shots follow selected retention.
            let removed = db.purge_auto_screen_frames()?;
            if removed > 0 {
                eprintln!("[visual_recall] disabled — purged {removed} auto frame(s)");
            }
        }
        eprintln!(
            "[visual_recall] screen OCR {}",
            if enabled { "enabled" } else { "off" }
        );
        Ok(())
    }

    #[tauri::command]
    pub fn set_visual_recall_retention(
        retention_days: u8,
        app: tauri::AppHandle,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Result<(), String> {
        if !valid_retention_days(retention_days) {
            return Err("retention must be 1, 3, 5, or 7 days".to_string());
        }
        let candidate = {
            let lane = LANE.lock().map_err(|_| "busy".to_string())?;
            let shared = lane.as_ref().ok_or_else(|| "not ready".to_string())?;
            let current = shared.read().map_err(|_| "busy".to_string())?;
            Settings {
                retention_days,
                ..current.clone()
            }
        };
        save(&app, &candidate)?;
        {
            let lane = LANE.lock().map_err(|_| "busy".to_string())?;
            let shared = lane.as_ref().ok_or_else(|| "not ready".to_string())?;
            *shared.write().map_err(|_| "busy".to_string())? = candidate.clone();
        }
        let removed = db.purge_screen_frames(candidate.retention_ms())?;
        eprintln!(
            "[visual_recall] retention set to {retention_days} day(s); purged {removed} expired frame(s)"
        );
        Ok(())
    }

    #[derive(serde::Serialize)]
    pub struct VisualRecallSnippet {
        pub ts: i64,
        pub app: Option<String>,
        pub window: Option<String>,
        pub chars: usize,
        pub dwell_ms: i64,
        pub display_id: Option<i64>,
        pub excerpt: String,
    }

    #[derive(serde::Serialize)]
    pub struct VisualRecallStatus {
        pub enabled: bool,
        pub retention_days: u8,
        pub estimated_daily_bytes: i64,
        pub events_24h: i64,
        pub frames_count: i64,
        pub frames_oldest_ms: Option<i64>,
        pub frames_bytes: i64,
        pub recent: Vec<VisualRecallSnippet>,
    }

    #[tauri::command]
    pub fn get_visual_recall_status(
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> VisualRecallStatus {
        const PREVIEW_CHARS: usize = 140;
        let settings = get_visual_recall_settings();
        let enabled = settings.enabled;
        let now = db.now_ms();
        let estimated_daily_bytes = db.screen_frames_bytes_in_range(
            now.saturating_sub(shogun_core::capture::visual_recall::DAY_MS),
            now,
        );
        let frame_stats = db.screen_frame_stats();
        let recent = db
            .screen_ocr_previews(5, PREVIEW_CHARS)
            .into_iter()
            .map(|p| VisualRecallSnippet {
                ts: p.ts,
                app: p.app_bundle_id,
                window: p.window_title,
                chars: p.content_len,
                dwell_ms: p.dwell_ms,
                display_id: p.display_id,
                excerpt: p.excerpt,
            })
            .collect();
        VisualRecallStatus {
            enabled,
            retention_days: settings.retention_days,
            estimated_daily_bytes,
            events_24h: db.screen_ocr_count_24h(),
            frames_count: frame_stats.count,
            frames_oldest_ms: frame_stats.oldest_ms,
            frames_bytes: frame_stats.total_bytes,
            recent,
        }
    }

    #[derive(serde::Serialize)]
    pub struct FrameListItem {
        pub id: i64,
        pub ts: i64,
        pub event_id: i64,
        pub app: Option<String>,
        pub window: Option<String>,
        pub width: u32,
        pub height: u32,
        pub jpeg_bytes: usize,
        pub ocr_excerpt: String,
        pub source: String,
    }

    #[tauri::command]
    pub fn list_screen_frames(db: tauri::State<'_, shogun_core::daemon::Db>) -> Vec<FrameListItem> {
        const LIMIT: usize = 200;
        const EXCERPT: usize = 160;
        let retention_ms = get_visual_recall_settings().retention_ms();
        db.list_screen_frames(retention_ms, LIMIT)
            .into_iter()
            .map(|s| FrameListItem {
                id: s.id,
                ts: s.created_at_ms,
                event_id: s.event_id,
                app: s.app_bundle_id,
                window: s.window_title,
                width: s.width,
                height: s.height,
                jpeg_bytes: s.jpeg_bytes,
                ocr_excerpt: shogun_memory::search::excerpt(&s.ocr_text, "", EXCERPT),
                source: s.source,
            })
            .collect()
    }

    #[derive(serde::Serialize)]
    pub struct FrameImage {
        pub id: i64,
        pub mime: String,
        pub width: u32,
        pub height: u32,
        pub jpeg_base64: String,
        pub ocr_text: String,
        pub ts: i64,
        pub app: Option<String>,
        pub window: Option<String>,
        pub source: String,
    }

    #[tauri::command]
    pub fn get_screen_frame_image(
        frame_id: i64,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Result<FrameImage, String> {
        let rec = db
            .get_screen_frame(frame_id)
            .ok_or_else(|| "not found".to_string())?;
        let s = rec.summary;
        Ok(FrameImage {
            id: s.id,
            mime: "image/jpeg".to_string(),
            width: s.width,
            height: s.height,
            jpeg_base64: base64_encode(&rec.jpeg),
            ocr_text: s.ocr_text,
            ts: s.created_at_ms,
            app: s.app_bundle_id,
            window: s.window_title,
            source: s.source,
        })
    }

    #[tauri::command]
    pub fn delete_screen_frame(
        frame_id: i64,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Result<(), String> {
        if db.delete_screen_frame(frame_id)? {
            Ok(())
        } else {
            Err("not found".into())
        }
    }

    #[tauri::command]
    pub fn open_visual_recall(app: tauri::AppHandle) {
        crate::build_visual_recall_window(&app);
    }

    fn base64_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TABLE[(b2 & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}
