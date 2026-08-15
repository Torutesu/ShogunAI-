//! The daemon's Memory API data backend (§6.11). Feature `db`. Implements shogun-mcp's
//! [`MemoryBackend`] over the daemon's [`Db`], so the REST / CLI / MCP faces read real state.
//!
//! Dependency-inverted: the trait is owned by the API layer (shogun-mcp) and implemented here by
//! the daemon — the API layer never depends on the core. The confidence gate stays in the API
//! layer (FR-API-06); this backend only supplies rows.

use std::path::{Path, PathBuf};

use serde_json::json;
use shogun_mcp::backend::{MemoryBackend, ReadItem, ReadParams, WriteResult};
use shogun_mcp::memory_api::Tool;

use crate::capture::visual_recall::{load_settings, save_settings, Settings};
use crate::daemon::{local_day_bounds, Db};
use shogun_mcp::memory_api_settings::{
    load_settings as load_memory_api_settings, save_settings as save_memory_api_settings,
    Settings as MemoryApiSettings,
};

/// Max search hits returned by `memory.search` over the API.
const SEARCH_LIMIT: usize = 20;
/// Max frame hits for `visual_recall.search_frames`.
const FRAME_SEARCH_LIMIT: usize = 20;
/// OCR excerpt length for frame search / status previews.
const FRAME_EXCERPT_CHARS: usize = 200;
/// Recent durable activity included in query-free `memory.get_context` snapshots.
const CONTEXT_ACTIVITY_LIMIT: usize = 8;
/// Per-activity text cap keeps the snapshot compact even when a captured window is huge.
const CONTEXT_ACTIVITY_EXCERPT_CHARS: usize = 320;
/// Confidence assigned to a captured event in search results: events are ground truth, not inferred
/// state, so they always pass the confidence gate (they are not "possibly").
const EVENT_CONFIDENCE: f64 = 1.0;

/// A [`MemoryBackend`] backed by the daemon's DB handle.
pub struct DbBackend {
    db: Db,
    visual_recall_settings_path: Option<PathBuf>,
    memory_api_settings_path: Option<PathBuf>,
}

impl DbBackend {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            visual_recall_settings_path: None,
            memory_api_settings_path: None,
        }
    }

    /// Path to `visual_recall.json` (same directory as `memory.db` in the desktop app).
    pub fn with_visual_recall_settings_path(mut self, path: PathBuf) -> Self {
        self.visual_recall_settings_path = Some(path);
        self
    }

    /// Path to `memory_api.json` (enable gate + profile prefs).
    pub fn with_memory_api_settings_path(mut self, path: PathBuf) -> Self {
        self.memory_api_settings_path = Some(path);
        self
    }

    fn visual_recall_settings_path(&self) -> Option<&Path> {
        self.visual_recall_settings_path.as_deref()
    }

    fn memory_api_settings_path(&self) -> Option<&Path> {
        self.memory_api_settings_path.as_deref()
    }

    fn load_vr_settings(&self) -> Settings {
        self.visual_recall_settings_path()
            .map(load_settings)
            .unwrap_or_default()
    }

    fn save_vr_settings(&self, settings: &Settings) -> Result<(), String> {
        let Some(path) = self.visual_recall_settings_path() else {
            return Err("visual_recall settings path not configured".to_string());
        };
        save_settings(path, settings)
    }

    fn load_ma_settings(&self) -> MemoryApiSettings {
        self.memory_api_settings_path()
            .map(load_memory_api_settings)
            .unwrap_or_default()
    }

    fn save_ma_settings(&self, settings: &MemoryApiSettings) -> Result<(), String> {
        let Some(path) = self.memory_api_settings_path() else {
            return Err("memory_api settings path not configured".to_string());
        };
        save_memory_api_settings(path, settings)
    }

    /// DB-derived context for agents. Live AX Notch cache is not available to standalone MCP.
    fn get_context_items(&self) -> Vec<ReadItem> {
        let mut items = Vec::new();
        items.push(ReadItem::new(
            "note: live AX Notch context cache is not available to standalone Memory API / MCP; this snapshot is DB-derived only"
                .to_string(),
            EVENT_CONFIDENCE,
        ));
        for fact in self.db.inline_memory(12) {
            items.push(ReadItem::new(fact, 0.85));
        }
        for note in self.db.recent_user_notes(8) {
            items.push(ReadItem::new(format!("note: {note}"), EVENT_CONFIDENCE));
        }
        for (source, event) in self
            .db
            .recent_context_previews(CONTEXT_ACTIVITY_LIMIT, CONTEXT_ACTIVITY_EXCERPT_CHARS)
        {
            let location = event
                .window_title
                .filter(|title| !title.trim().is_empty())
                .or(event.app_bundle_id)
                .map(|value| format!(", {value}"))
                .unwrap_or_default();
            items.push(ReadItem::new(
                format!("recent activity [{source}{location}]: {}", event.excerpt),
                EVENT_CONFIDENCE,
            ));
        }
        items
    }

    fn whoami_json(&self) -> String {
        let profile = self.load_ma_settings().profile;
        let people: Vec<_> = self.db.people().into_iter().map(|p| p.display_name).collect();
        let projects: Vec<_> = self.db.projects().into_iter().map(|p| p.name).collect();
        let commitments: Vec<_> = self
            .db
            .commitments_due(self.db.now_ms())
            .into_iter()
            .map(|c| c.description)
            .collect();
        let open_loops: Vec<_> = self.db.open_loops().into_iter().map(|o| o.description).collect();
        let work = json!({
            "people": { "count": people.len(), "names": people },
            "projects": { "count": projects.len(), "names": projects },
            "commitments": { "count": commitments.len(), "names": commitments },
            "open_loops": { "count": open_loops.len(), "names": open_loops },
        });
        json!({
            "profile": {
                "display_name": profile.display_name,
                "role": profile.role,
                "prefs": profile.prefs,
            },
            "work": work,
        })
        .to_string()
    }

    fn apply_profile_set(&self, body: &str) -> Result<(), String> {
        let v: serde_json::Value =
            serde_json::from_str(body).map_err(|_| "expected profile JSON object".to_string())?;
        let mut settings = self.load_ma_settings();
        let patch = |field: &mut String, key: &str| {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                *field = s.to_string();
            }
        };
        patch(&mut settings.profile.display_name, "display_name");
        patch(&mut settings.profile.role, "role");
        patch(&mut settings.profile.prefs, "prefs");
        // Allow nested { "profile": { ... } } as well.
        if let Some(p) = v.get("profile") {
            if let Some(s) = p.get("display_name").and_then(|x| x.as_str()) {
                settings.profile.display_name = s.to_string();
            }
            if let Some(s) = p.get("role").and_then(|x| x.as_str()) {
                settings.profile.role = s.to_string();
            }
            if let Some(s) = p.get("prefs").and_then(|x| x.as_str()) {
                settings.profile.prefs = s.to_string();
            }
        }
        self.save_ma_settings(&settings)
    }

    fn visual_recall_status_json(&self) -> String {
        let settings = self.load_vr_settings();
        let enabled = settings.enabled;
        let now = self.db.now_ms();
        let ms_24h = crate::capture::visual_recall::DAY_MS;
        let retention_ms = settings.retention_ms();
        let frame_stats = self.db.screen_frame_stats();
        let frames_24h = self.db.screen_frames_count_in_range(now - ms_24h, now);
        let frames_retained = self.db.screen_frames_count_in_range(now - retention_ms, now);
        let estimated_daily_bytes = self.db.screen_frames_bytes_in_range(now - ms_24h, now);
        let recent: Vec<_> = self
            .db
            .screen_ocr_previews(5, 140)
            .into_iter()
            .map(|p| {
                json!({
                    "ts": p.ts,
                    "app": p.app_bundle_id,
                    "window": p.window_title,
                    "chars": p.content_len,
                    "dwell_ms": p.dwell_ms,
                    "display_id": p.display_id,
                    "excerpt": p.excerpt,
                })
            })
            .collect();
        let last_capture = self.db.latest_screen_frame_summary().map(|s| {
            json!({
                "frame_id": s.id,
                "ts": s.created_at_ms,
                "event_id": s.event_id,
                "app": s.app_bundle_id,
                "window": s.window_title,
                "width": s.width,
                "height": s.height,
                "jpeg_bytes": s.jpeg_bytes,
                "ocr_chars": s.ocr_text.chars().count(),
            })
        });
        json!({
            "enabled": enabled,
            "retention_days": settings.retention_days,
            "estimated_daily_bytes": estimated_daily_bytes,
            "events_24h": self.db.screen_ocr_count_24h(),
            "frames_count": frame_stats.count,
            "frames_24h": frames_24h,
            "frames_retained": frames_retained,
            "frames_oldest_ms": frame_stats.oldest_ms,
            "frames_bytes": frame_stats.total_bytes,
            "last_capture": last_capture,
            "recent": recent,
        })
        .to_string()
    }

    fn visual_recall_search_json(&self, params: &ReadParams) -> String {
        let query = params.query.as_deref().unwrap_or("");
        let now = self.db.now_ms();
        let retention_ms = self.load_vr_settings().retention_ms();
        let local_days = local_day_bounds(now);
        let (from_ms, to_ms) = match (params.from_ms, params.to_ms) {
            (Some(f), Some(t)) => (f, t),
            (Some(f), None) => (f, now),
            (None, Some(t)) => (0, t),
            (None, None) if query.trim().is_empty() => (now - retention_ms, now),
            (None, None) => {
                shogun_memory::search::visual_recall_window(query, now, local_days, retention_ms)
            }
        };
        let hits = self.db.search_screen_frames_window(query, from_ms, to_ms, FRAME_SEARCH_LIMIT, FRAME_EXCERPT_CHARS);
        let frames: Vec<_> = hits
            .iter()
            .map(|f| {
                json!({
                    "frame_id": f.frame_id,
                    "event_id": f.event_id,
                    "ts": f.ts,
                    "app": f.app_bundle_id,
                    "window": f.window_title,
                    "width": f.width,
                    "height": f.height,
                    "ocr_excerpt": f.ocr_excerpt,
                    "needs_rescan": f.needs_rescan,
                    "source": f.source,
                })
            })
            .collect();
        json!({
            "query": query,
            "from_ms": from_ms,
            "to_ms": to_ms,
            "frames": frames,
        })
        .to_string()
    }

    fn visual_recall_get_frame_json(&self, params: &ReadParams) -> String {
        let Some(frame_id) = params.id else {
            return r#"{"error":"missing_frame_id"}"#.to_string();
        };
        let Some(s) = self.db.get_screen_frame_summary(frame_id) else {
            return json!({ "error": "not_found", "frame_id": frame_id }).to_string();
        };
        let needs_rescan =
            s.ocr_text.trim().len() < shogun_memory::screen_frames::THIN_OCR_CHARS;
        json!({
            "frame_id": s.id,
            "event_id": s.event_id,
            "ts": s.created_at_ms,
            "app": s.app_bundle_id,
            "window": s.window_title,
            "display_id": s.display_id,
            "width": s.width,
            "height": s.height,
            "jpeg_bytes": s.jpeg_bytes,
            "ocr_text": s.ocr_text,
            "needs_rescan": needs_rescan,
            "source": s.source,
        })
        .to_string()
    }

    fn visual_recall_rescan_json(&self, params: &ReadParams) -> String {
        let Some(frame_id) = params.id else {
            return r#"{"error":"missing_frame_id"}"#.to_string();
        };
        let Some(rec) = self.db.get_screen_frame(frame_id) else {
            return json!({ "error": "not_found", "frame_id": frame_id }).to_string();
        };
        match crate::capture::visual_recall::ocr_jpeg_bytes(&rec.jpeg) {
            Some(text) => {
                let updated = match self.db.update_event_ocr_text(rec.summary.event_id, &text) {
                    Ok(v) => v,
                    Err(_) => {
                        return json!({ "error": "event_update_failed", "frame_id": frame_id })
                            .to_string();
                    }
                };
                if !updated {
                    return json!({ "error": "event_update_failed", "frame_id": frame_id }).to_string();
                }
                let excerpt = shogun_memory::search::excerpt(&text, "", FRAME_EXCERPT_CHARS);
                json!({
                    "frame_id": frame_id,
                    "ocr_text": text,
                    "chars": text.chars().count(),
                    "excerpt": excerpt,
                })
                .to_string()
            }
            None => json!({
                "error": "ocr_failed",
                "frame_id": frame_id,
                "ocr_available": cfg!(target_os = "macos"),
            })
            .to_string(),
        }
    }
}

impl MemoryBackend for DbBackend {
    fn read(&self, tool: Tool, params: &ReadParams) -> Vec<ReadItem> {
        // A single row `Option` → a 0/1-length result (for the `get` variants).
        fn one<T>(row: Option<T>, f: impl Fn(T) -> ReadItem) -> Vec<ReadItem> {
            row.into_iter().map(f).collect()
        }
        let id = params.id;

        match tool {
            Tool::MemorySearch => self
                .db
                .search(params.query.as_deref().unwrap_or(""), SEARCH_LIMIT)
                .into_iter()
                .map(|hit| ReadItem::new(hit.content, EVENT_CONFIDENCE))
                .collect(),
            // DB-derived work context (AX Notch cache is not available to standalone MCP).
            Tool::MemoryGetContext => self.get_context_items(),

            Tool::StatePeopleList => {
                self.db.people().into_iter().map(|p| ReadItem::new(p.display_name, p.confidence)).collect()
            }
            Tool::StatePeopleGet => {
                one(id.and_then(|i| self.db.person(i)), |p| ReadItem::new(p.display_name, p.confidence))
            }
            Tool::StateProjectsList => {
                self.db.projects().into_iter().map(|p| ReadItem::new(p.name, p.confidence)).collect()
            }
            Tool::StateProjectsGet => {
                one(id.and_then(|i| self.db.project(i)), |p| ReadItem::new(p.name, p.confidence))
            }
            // Commitments/open loops reuse the Fusion supply. `now` from the daemon clock so
            // `overdue` is consistent with the rest of the daemon.
            Tool::StateCommitmentsList => self
                .db
                .commitments_due(self.db.now_ms())
                .into_iter()
                .map(|c| ReadItem::new(c.description, c.confidence))
                .collect(),
            Tool::StateCommitmentsGet => {
                one(id.and_then(|i| self.db.commitment(i)), |c| ReadItem::new(c.description, c.confidence))
            }
            Tool::StateOpenLoopsList => {
                self.db.open_loops().into_iter().map(|o| ReadItem::new(o.description, o.confidence)).collect()
            }
            Tool::StateOpenLoopsGet => {
                one(id.and_then(|i| self.db.open_loop(i)), |o| ReadItem::new(o.description, o.confidence))
            }

            // Structured visual-recall / profile tools use [`Self::read_structured`].
            Tool::VisualRecallStatus
            | Tool::VisualRecallSearchFrames
            | Tool::VisualRecallGetFrame
            | Tool::VisualRecallRescanFrame
            | Tool::ProfileWhoami
            | Tool::MemoryAppendNote
            | Tool::StateProposeUpdate
            | Tool::VisualRecallSetEnabled
            | Tool::VisualRecallSetRetention
            | Tool::VisualRecallDeleteFrame
            | Tool::ProfileSet
            | Tool::ActionsExecute => Vec::new(),
        }
    }

    fn read_structured(&self, tool: Tool, params: &ReadParams) -> Option<String> {
        Some(match tool {
            Tool::VisualRecallStatus => self.visual_recall_status_json(),
            Tool::VisualRecallSearchFrames => self.visual_recall_search_json(params),
            Tool::VisualRecallGetFrame => self.visual_recall_get_frame_json(params),
            Tool::VisualRecallRescanFrame => self.visual_recall_rescan_json(params),
            Tool::ProfileWhoami => self.whoami_json(),
            _ => return None,
        })
    }

    fn write(&self, tool: Tool, body: &str) -> WriteResult {
        match tool {
            // Persist the note to the event log (L1, reversible).
            Tool::MemoryAppendNote => match self.db.append_note(body) {
                Some(id) => Ok(Some(id)),
                None => Err("append_note failed".to_string()),
            },
            // A proposed state change is accepted here and surfaces in the Notch for L2 confirm; a
            // proposals table is future work, so nothing is persisted yet.
            Tool::StateProposeUpdate => Ok(None),
            Tool::VisualRecallSetEnabled => {
                let enabled = parse_enabled_body(body)?;
                let settings = Settings { enabled, ..self.load_vr_settings() };
                self.save_vr_settings(&settings)?;
                if !enabled {
                    let removed = self.db.purge_auto_screen_frames()?;
                    if removed > 0 {
                        eprintln!("[visual_recall] disabled via API — purged {removed} auto frame(s)");
                    }
                }
                Ok(None)
            }
            Tool::VisualRecallSetRetention => {
                let retention_days = parse_retention_body(body)?;
                let settings = Settings {
                    retention_days,
                    ..self.load_vr_settings()
                };
                self.save_vr_settings(&settings)?;
                self.db.purge_screen_frames(settings.retention_ms())?;
                Ok(None)
            }
            Tool::VisualRecallDeleteFrame => {
                let id = parse_id_body(body)?;
                if self.db.delete_screen_frame(id)? {
                    Ok(None)
                } else {
                    Err("frame not found".to_string())
                }
            }
            Tool::ProfileSet => {
                self.apply_profile_set(body)?;
                Ok(None)
            }
            // Not a write tool.
            _ => Err("not a write tool".to_string()),
        }
    }
}

fn parse_id_body(body: &str) -> Result<i64, String> {
    if let Ok(id) = body.trim().parse::<i64>() {
        return Ok(id);
    }
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| "expected frame id".to_string())?;
    v.get("id")
        .and_then(|x| x.as_i64())
        .ok_or_else(|| "expected {\"id\":number}".to_string())
}

fn parse_enabled_body(body: &str) -> Result<bool, String> {
    if body.trim() == "true" {
        return Ok(true);
    }
    if body.trim() == "false" {
        return Ok(false);
    }
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| "expected {\"enabled\":bool}".to_string())?;
    v.get("enabled")
        .and_then(|x| x.as_bool())
        .ok_or_else(|| "expected {\"enabled\":bool}".to_string())
}

fn parse_retention_body(body: &str) -> Result<u8, String> {
    let parsed = body.trim().parse::<u8>().ok().or_else(|| {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()?
            .get("retention_days")?
            .as_u64()
            .and_then(|days| u8::try_from(days).ok())
    });
    let days = parsed.ok_or_else(|| "expected {\"retention_days\":1|3|5|7}".to_string())?;
    if crate::capture::visual_recall::valid_retention_days(days) {
        Ok(days)
    } else {
        Err("retention must be 1, 3, 5, or 7 days".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_memory::state::{CommitmentDirection, CommitmentStatus, NewCommitment, Provenance};
    use std::sync::Arc;

    fn ev<'a>(hash: &'a str) -> shogun_memory::event_log::NewEvent<'a> {
        shogun_memory::event_log::NewEvent {
            ts: 1,
            source: "capture",
            kind: "text",
            app_bundle_id: None,
            window_title: None,
            content: "evidence",
            content_hash: hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        }
    }

    fn seed() -> Db {
        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let (e, _) = db.capture(&ev("h1")).unwrap();
        db.insert_commitment(
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send the report",
                due_at: Some(50),
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[Provenance::new(e)],
        )
        .unwrap();
        db
    }

    fn backend_with_settings(db: Db) -> (DbBackend, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "shogun_vr_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let ma = dir.join("memory_api.json");
        let backend = DbBackend::new(db)
            .with_visual_recall_settings_path(dir.join("visual_recall.json"))
            .with_memory_api_settings_path(ma.clone());
        (backend, ma)
    }

    fn params() -> ReadParams {
        ReadParams::default()
    }
    fn get(id: i64) -> ReadParams {
        ReadParams { id: Some(id), query: None, from_ms: None, to_ms: None }
    }

    #[test]
    fn backend_reads_commitments_from_the_db() {
        let backend = DbBackend::new(seed());
        let items = backend.read(Tool::StateCommitmentsList, &params());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "send the report");
        assert_eq!(items[0].confidence, 0.9);
    }

    #[test]
    fn backend_reads_people_list_and_get() {
        use shogun_memory::state::{NewPerson, Provenance};
        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        let (e, _) = db.capture(&ev("h1")).unwrap();
        let id = db
            .insert_person(&NewPerson { display_name: "Alice", confidence: 0.85, now: 1, ..Default::default() }, &[Provenance::new(e)])
            .unwrap();
        let backend = DbBackend::new(db);

        let list = backend.read(Tool::StatePeopleList, &params());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "Alice");

        let got = backend.read(Tool::StatePeopleGet, &get(id));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "Alice");

        // a missing id → empty
        assert!(backend.read(Tool::StatePeopleGet, &get(9999)).is_empty());
    }

    #[test]
    fn backend_search_returns_event_content_as_ground_truth() {
        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        db.capture(&shogun_memory::event_log::NewEvent {
            ts: 1,
            source: "capture",
            kind: "text",
            app_bundle_id: None,
            window_title: None,
            content: "the quarterly roadmap review",
            content_hash: "h1",
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
        let backend = DbBackend::new(db);
        let hits = backend.read(
            Tool::MemorySearch,
            &ReadParams {
                id: None,
                query: Some("roadmap".into()),
                from_ms: None,
                to_ms: None,
            },
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].label.contains("roadmap"));
        assert_eq!(hits[0].confidence, EVENT_CONFIDENCE); // events always pass the gate
        // empty query → no search
        assert!(backend.read(Tool::MemorySearch, &params()).is_empty());
    }

    #[test]
    fn write_and_action_tools_are_not_reads() {
        let backend = DbBackend::new(seed());
        assert!(backend.read(Tool::MemoryAppendNote, &params()).is_empty());
        assert!(backend.read(Tool::ActionsExecute, &params()).is_empty());
    }

    #[test]
    fn append_note_persists_to_the_event_log() {
        let db = Db::open_in_memory(Arc::new(|| 555)).unwrap();
        let backend = DbBackend::new(db.clone());
        let id = match backend.write(Tool::MemoryAppendNote, "remember to call Alice") {
            Ok(Some(id)) => id,
            other => panic!("expected a persisted note id, got {other:?}"),
        };
        assert!(id > 0);
        // it's a searchable user note in the event log
        let hits = backend.read(
            Tool::MemorySearch,
            &ReadParams {
                id: None,
                query: Some("Alice".into()),
                from_ms: None,
                to_ms: None,
            },
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].label.contains("call Alice"));
    }

    #[test]
    fn propose_is_accepted_without_persistence_and_non_writes_error() {
        let backend = DbBackend::new(seed());
        assert_eq!(backend.write(Tool::StateProposeUpdate, "{}"), Ok(None));
        assert!(backend.write(Tool::MemorySearch, "x").is_err());
    }

    #[test]
    fn get_context_includes_user_notes() {
        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        let backend = DbBackend::new(db.clone());
        assert!(backend.write(Tool::MemoryAppendNote, "remember the launch checklist").is_ok());
        let items = backend.read(Tool::MemoryGetContext, &params());
        assert!(items.len() >= 2, "expected disclaimer + note, got {items:?}");
        assert!(items.iter().any(|i| i.label.contains("launch checklist")), "{items:?}");
        assert!(items.iter().any(|i| i.label.contains("AX Notch")), "{items:?}");
    }

    #[test]
    fn get_context_includes_recent_durable_activity() {
        let db = Db::open_in_memory(Arc::new(|| 2)).unwrap();
        db.capture(&shogun_memory::event_log::NewEvent {
            ts: 2,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.example.browser"),
            window_title: Some("Roadmap"),
            content: "quarterly roadmap review has three open decisions",
            content_hash: "roadmap-context",
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
        let backend = DbBackend::new(db);

        let items = backend.read(Tool::MemoryGetContext, &params());

        assert!(items.iter().any(|item| {
            item.label.contains("recent activity [capture, Roadmap]")
                && item.label.contains("three open decisions")
        }), "{items:?}");
    }

    #[test]
    fn whoami_returns_profile_and_work_summary() {
        use shogun_memory::state::{NewPerson, Provenance};
        use shogun_mcp::memory_api_settings::{save_settings, Profile, Settings as MaSettings};

        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        let (e, _) = db.capture(&ev("h1")).unwrap();
        db.insert_person(
            &NewPerson { display_name: "Alice", confidence: 0.9, now: 1, ..Default::default() },
            &[Provenance::new(e)],
        )
        .unwrap();
        let (backend, path) = backend_with_settings(db);
        save_settings(
            &path,
            &MaSettings {
                enabled: true,
                profile: Profile {
                    display_name: "Anant".into(),
                    role: "founder".into(),
                    prefs: "prefer bullet answers".into(),
                },
            },
        )
        .unwrap();
        let json = backend.read_structured(Tool::ProfileWhoami, &params()).expect("whoami");
        assert!(json.contains("Anant"), "{json}");
        assert!(json.contains("founder"), "{json}");
        assert!(json.contains("Alice"), "{json}");
        assert!(json.contains("prefer bullet answers"), "{json}");
    }

    #[test]
    fn profile_set_persists_to_settings() {
        let db = Db::open_in_memory(Arc::new(|| 1)).unwrap();
        let (backend, _) = backend_with_settings(db);
        assert!(backend
            .write(Tool::ProfileSet, r#"{"display_name":"Kai","role":"eng","prefs":"short"}"#)
            .is_ok());
        let json = backend.read_structured(Tool::ProfileWhoami, &params()).expect("whoami");
        assert!(json.contains("Kai") && json.contains("eng") && json.contains("short"), "{json}");
    }

    #[test]
    fn visual_recall_status_and_set_enabled() {
        let db = Db::open_in_memory(Arc::new(|| 5_000)).unwrap();
        let (backend, _) = backend_with_settings(db);
        let status = backend.read_structured(Tool::VisualRecallStatus, &params()).expect("status");
        assert!(status.contains("\"enabled\":false"));
        assert!(status.contains("\"retention_days\":3"));
        assert!(backend.write(Tool::VisualRecallSetEnabled, r#"{"enabled":true}"#).is_ok());
        let status2 = backend.read_structured(Tool::VisualRecallStatus, &params()).expect("status");
        assert!(status2.contains("\"enabled\":true"));
        assert!(backend
            .write(Tool::VisualRecallSetRetention, r#"{"retention_days":7}"#)
            .is_ok());
        let status3 = backend.read_structured(Tool::VisualRecallStatus, &params()).expect("status");
        assert!(status3.contains("\"retention_days\":7"));
        assert!(backend
            .write(Tool::VisualRecallSetRetention, r#"{"retention_days":30}"#)
            .is_err());
    }

    #[test]
    fn full_stack_note_write_through_rest_respond_with() {
        use shogun_mcp::memory_api::TokenRegistry;
        use shogun_mcp::rest::{respond_with, Method, RestRequest};

        let backend = DbBackend::new(Db::open_in_memory(Arc::new(|| 1)).unwrap());
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let req = RestRequest {
            method: Method::Post,
            path: "/v1/memory/notes".into(),
            token: Some("t".into()),
            include_low: false,
            query: None,
            body: Some("ship v1 on friday".into()),
            from_ms: None,
            to_ms: None,
        };
        let (status, body) = respond_with(&req, &tokens, &backend);
        assert_eq!(status, 202);
        assert!(body.contains("memory.append_note"));
        assert!(body.contains("\"level\":\"L1\""));
        assert!(body.contains("\"id\":"), "persisted note id missing: {body}");
    }

    #[test]
    fn full_stack_db_backend_through_rest_respond_with() {
        use shogun_mcp::memory_api::TokenRegistry;
        use shogun_mcp::rest::{respond_with, Method, RestRequest};

        let backend = DbBackend::new(seed());
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let req = RestRequest {
            method: Method::Get,
            path: "/v1/state/commitments".into(),
            token: Some("t".into()),
            include_low: false,
            query: None,
            body: None,
            from_ms: None,
            to_ms: None,
        };
        let (status, body) = respond_with(&req, &tokens, &backend);
        assert_eq!(status, 200);
        // real DB data rendered through the API layer's confidence-gated JSON
        assert!(body.contains("send the report"), "body: {body}");
        assert!(body.contains("state.commitments.list"));
    }
}
