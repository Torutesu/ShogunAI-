//! Visual recall settings + Tauri commands (issue #106/#107).
//!
//! Opt-in screen OCR (default off). Persisted to `visual_recall.json` under app data; the capture
//! poller reads the shared `RwLock` each tick so toggling applies immediately.
//!
//! Compressed JPEG frames (≤72 h) are stored in the memory DB when enabled; recall APIs let chat
//! pull frames by query/time and re-OCR on demand.

#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod ocr_gate;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod pipeline;
#[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
pub mod text_regions;

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::{Mutex, RwLock};

    use shogun_core::capture::visual_recall::Settings;
    use tauri::Manager;

    pub type SharedSettings = std::sync::Arc<RwLock<Settings>>;

    static LANE: Mutex<Option<SharedSettings>> = Mutex::new(None);

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("visual_recall.json"))
    }

    /// Load persisted settings, publish the shared handle, return it for the capture poller.
    pub fn init(app: &tauri::AppHandle) -> SharedSettings {
        let mut settings = Settings::default();
        if let Some(p) = settings_path(app) {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(saved) = serde_json::from_str::<Settings>(&text) {
                    settings = saved;
                }
            }
        }
        let shared = std::sync::Arc::new(RwLock::new(settings.clone()));
        if let Ok(mut g) = LANE.lock() {
            *g = Some(shared.clone());
        }
        eprintln!(
            "[visual_recall] screen OCR {}",
            if settings.enabled { "enabled" } else { "off (default)" }
        );
        shared
    }

    fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(p) = settings_path(app) else { return Ok(()) };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("save failed: {e}"))
    }

    #[tauri::command]
    pub fn get_visual_recall_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.read().ok().map(|v| v.clone())))
            .flatten()
            .unwrap_or_default()
    }

    /// Master switch for screen OCR (issue #107). Persist first, then apply.
    #[tauri::command]
    pub fn set_visual_recall_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(shared) = g.as_ref() else { return Err("not ready".into()) };
            let current = shared.read().map_err(|_| "busy".to_string())?;
            Settings { enabled, ..current.clone() }
        };
        save(&app, &candidate)?;
        let Ok(g) = LANE.lock() else { return Err("busy".into()) };
        let Some(shared) = g.as_ref() else { return Err("not ready".into()) };
        let mut live = shared.write().map_err(|_| "busy".to_string())?;
        *live = candidate;
        eprintln!(
            "[visual_recall] screen OCR {}",
            if enabled { "enabled" } else { "off" }
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
        pub events_24h: i64,
        pub frames_count: i64,
        pub frames_oldest_ms: Option<i64>,
        pub frames_bytes: i64,
        pub recent: Vec<VisualRecallSnippet>,
    }

    /// Settings + live timeline for visual recall (issue #106). Text excerpts in `recent`; frame
    /// stats are counts only — no pixels over IPC.
    #[tauri::command]
    pub fn get_visual_recall_status(db: tauri::State<'_, shogun_core::daemon::Db>) -> VisualRecallStatus {
        const PREVIEW_CHARS: usize = 140;
        let enabled = get_visual_recall_settings().enabled;
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
            events_24h: db.screen_ocr_count_24h(),
            frames_count: frame_stats.count,
            frames_oldest_ms: frame_stats.oldest_ms,
            frames_bytes: frame_stats.total_bytes,
            recent,
        }
    }

    #[derive(serde::Serialize)]
    pub struct ScreenFrameView {
        pub id: i64,
        pub event_id: i64,
        pub ts: i64,
        pub app: Option<String>,
        pub window: Option<String>,
        pub width: u32,
        pub height: u32,
        pub jpeg_bytes: usize,
        pub ocr_text: String,
        /// Raw JPEG bytes (local-only; hook for future vision input).
        pub jpeg: Vec<u8>,
    }

    #[derive(serde::Serialize)]
    pub struct ScreenFrameSummaryView {
        pub id: i64,
        pub event_id: i64,
        pub ts: i64,
        pub app: Option<String>,
        pub window: Option<String>,
        pub width: u32,
        pub height: u32,
        pub jpeg_bytes: usize,
        pub ocr_excerpt: String,
        pub needs_rescan: bool,
    }

    fn frame_to_view(rec: shogun_memory::screen_frames::FrameRecord) -> ScreenFrameView {
        let s = rec.summary;
        let jpeg_len = rec.jpeg.len();
        ScreenFrameView {
            id: s.id,
            event_id: s.event_id,
            ts: s.created_at_ms,
            app: s.app_bundle_id,
            window: s.window_title,
            width: s.width,
            height: s.height,
            jpeg_bytes: jpeg_len,
            ocr_text: s.ocr_text,
            jpeg: rec.jpeg,
        }
    }

    /// Fetch one stored frame by id (JPEG + linked OCR text).
    #[tauri::command]
    pub fn get_screen_frame(
        frame_id: i64,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Option<ScreenFrameView> {
        db.get_screen_frame(frame_id).map(frame_to_view)
    }

    /// Search stored frames for a visual-recall question (metadata only — use [`get_screen_frame`] for bytes).
    #[tauri::command]
    pub fn search_screen_frames(
        query: String,
        limit: Option<usize>,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Vec<ScreenFrameSummaryView> {
        const EXCERPT: usize = 200;
        db.search_screen_frames(&query, limit.unwrap_or(6), EXCERPT)
            .into_iter()
            .map(|f| ScreenFrameSummaryView {
                id: f.frame_id,
                event_id: f.event_id,
                ts: f.ts,
                app: f.app_bundle_id,
                window: f.window_title,
                width: f.width,
                height: f.height,
                jpeg_bytes: 0,
                ocr_excerpt: f.ocr_excerpt,
                needs_rescan: f.needs_rescan,
            })
            .collect()
    }

    /// Frames in a time window (ms since epoch), newest first.
    #[tauri::command]
    pub fn get_screen_frames_in_range(
        from_ms: i64,
        to_ms: i64,
        limit: Option<usize>,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Vec<ScreenFrameSummaryView> {
        db.screen_frames_in_range(from_ms, to_ms, limit.unwrap_or(20))
            .into_iter()
            .map(|s| ScreenFrameSummaryView {
                id: s.id,
                event_id: s.event_id,
                ts: s.created_at_ms,
                app: s.app_bundle_id,
                window: s.window_title,
                width: s.width,
                height: s.height,
                jpeg_bytes: s.jpeg_bytes,
                ocr_excerpt: shogun_memory::search::excerpt(&s.ocr_text, "", 200),
                needs_rescan: s.ocr_text.trim().len() < shogun_memory::screen_frames::THIN_OCR_CHARS,
            })
            .collect()
    }

    /// Re-OCR a stored frame (pull image → Vision). Returns fresh text; does not mutate DB.
    #[cfg(feature = "visual-recall-ocr")]
    #[tauri::command]
    pub fn rescan_screen_frame(
        frame_id: i64,
        db: tauri::State<'_, shogun_core::daemon::Db>,
    ) -> Result<String, String> {
        let rec = db.get_screen_frame(frame_id).ok_or("frame not found")?;
        crate::screen_ocr::ocr_jpeg_bytes(&rec.jpeg)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| "Vision returned no text".to_string())
    }

    #[cfg(not(feature = "visual-recall-ocr"))]
    #[tauri::command]
    pub fn rescan_screen_frame(_frame_id: i64) -> Result<String, String> {
        Err("visual-recall-ocr feature disabled".to_string())
    }
}
