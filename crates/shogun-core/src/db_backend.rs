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

/// Max search hits returned by `memory.search` over the API.
const SEARCH_LIMIT: usize = 20;
/// Max frame hits for `visual_recall.search_frames`.
const FRAME_SEARCH_LIMIT: usize = 20;
/// OCR excerpt length for frame search / status previews.
const FRAME_EXCERPT_CHARS: usize = 200;
/// Evidence cap for `memory.get_context_pack` (FR-API-08) — matches the chat path's scale so the
/// pack an external AI receives is the same grounded slice the in-app chat reads.
const PACK_HITS: usize = 10;
/// Per-evidence excerpt cap for the pack, so one huge window capture cannot eat the whole pack.
const PACK_EXCERPT_CHARS: usize = 300;
/// Confidence assigned to a captured event in search results: events are ground truth, not inferred
/// state, so they always pass the confidence gate (they are not "possibly").
const EVENT_CONFIDENCE: f64 = 1.0;

/// A [`MemoryBackend`] backed by the daemon's DB handle.
pub struct DbBackend {
    db: Db,
    visual_recall_settings_path: Option<PathBuf>,
}

impl DbBackend {
    pub fn new(db: Db) -> Self {
        Self { db, visual_recall_settings_path: None }
    }

    /// Path to `visual_recall.json` (same directory as `memory.db` in the desktop app).
    pub fn with_visual_recall_settings_path(mut self, path: PathBuf) -> Self {
        self.visual_recall_settings_path = Some(path);
        self
    }

    fn visual_recall_settings_path(&self) -> Option<&Path> {
        self.visual_recall_settings_path.as_deref()
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

    fn visual_recall_status_json(&self) -> String {
        let enabled = self.load_vr_settings().enabled;
        let now = self.db.now_ms();
        let ms_24h = 24 * 60 * 60 * 1000;
        let ms_72h = shogun_memory::screen_frames::RETENTION_MS;
        let frame_stats = self.db.screen_frame_stats();
        let frames_24h = self.db.screen_frames_count_in_range(now - ms_24h, now);
        let frames_72h = self.db.screen_frames_count_in_range(now - ms_72h, now);
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
            "events_24h": self.db.screen_ocr_count_24h(),
            "frames_count": frame_stats.count,
            "frames_24h": frames_24h,
            "frames_72h": frames_72h,
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
        let local_days = local_day_bounds(now);
        let (from_ms, to_ms) = match (params.from_ms, params.to_ms) {
            (Some(f), Some(t)) => (f, t),
            (Some(f), None) => (f, now),
            (None, Some(t)) => (0, t),
            (None, None) if query.trim().is_empty() => (now - shogun_memory::screen_frames::RETENTION_MS, now),
            (None, None) => {
                shogun_memory::search::visual_recall_window(query, now, local_days)
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

    /// `lessons.list` (L5, Plan D-5): every lesson row, exactly what the Learned UI lists —
    /// and nothing more. `feedback_events` text has no path into this payload: the row type
    /// (`lessons::Lesson`) carries instructions and bookkeeping only.
    fn lessons_json(&self) -> String {
        let lessons: Vec<_> = self
            .db
            .lessons_all()
            .into_iter()
            .map(|l| {
                json!({
                    "id": l.id,
                    "kind": l.kind.as_str(),
                    "scope": l.scope.as_str(),
                    "scope_ref": l.scope_ref,
                    "instruction": l.instruction,
                    "confidence": l.confidence,
                    "evidence_count": l.evidence_count,
                    "active": l.active,
                })
            })
            .collect();
        json!({ "lessons": lessons }).to_string()
    }

    /// `memory.get_wrap` (issue #10, invariant 6): today's Evening Wrap as JSON — the same
    /// `Db::evening_wrap` aggregation the notch card draws, so the API face and the human face
    /// cannot disagree. Deterministic local aggregation only; the confidence gate is the
    /// assembler's (fusion), identical to the card's.
    fn evening_wrap_json(&self) -> String {
        let now = self.db.now_ms();
        let (day_start, tomorrow_end) = crate::daemon::local_wrap_window(now);
        let wrap = self.db.evening_wrap(Vec::new(), day_start, now, tomorrow_end);
        let line = |i: &shogun_fusion::brief::BriefItem| {
            json!({
                "text": i.text,
                "possibly": i.possibly,
                "provenance_event_id": i.provenance_event_id,
            })
        };
        json!({
            "date": crate::daemon::local_date_string(now),
            "outcome": {
                "commitments_done": wrap.outcome.commitments_done,
                "loops_closed": wrap.outcome.loops_closed,
                "actions_decided": wrap.outcome.actions_decided,
                "actions_adopted": wrap.outcome.actions_adopted,
            },
            "still_open": wrap.still_open.iter().map(line).collect::<Vec<_>>(),
            "tomorrow_calendar": wrap
                .tomorrow_calendar
                .iter()
                .map(|c| json!({ "start_ms": c.start_ms, "title": c.title, "updated": c.updated }))
                .collect::<Vec<_>>(),
            "tomorrow_commitments": wrap.tomorrow_commitments.iter().map(line).collect::<Vec<_>>(),
            "loose_ends": wrap.loose_ends.iter().map(line).collect::<Vec<_>>(),
        })
        .to_string()
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
            // `get_context` isn't a persisted read (the cache is RAM-only, AR-10) — empty here.
            Tool::MemoryGetContext => Vec::new(),

            // FR-API-08: the grounded context pack for one task/question — the same
            // `Db::assemble_context` the in-app chat uses (invariant 6), flattened to labeled
            // rows. Facts are already confidence-gated by `inline_memory` (medium carries its
            // `possibly:` prefix), and evidence rows are ground-truth events, so both pass the
            // API gate at event confidence; each evidence line carries its provenance inline
            // (event id, source, timestamp).
            //
            // `pack.screen_frames` is deliberately NOT flattened in here. Those are stored JPEGs
            // (the invariant-2 exception), and reaching them is its own separately-gated tool —
            // `visual_recall.get_frame`, which answers to Visual recall's on/off state. Handing
            // an API caller frame ids as part of a text pack would route around that switch.
            Tool::MemoryGetContextPack => {
                let query = params.query.as_deref().unwrap_or("");
                if query.is_empty() {
                    return Vec::new();
                }
                let pack = self.db.assemble_context(query, PACK_HITS, PACK_EXCERPT_CHARS);
                pack.facts
                    .into_iter()
                    .map(|f| ReadItem::new(format!("fact: {f}"), EVENT_CONFIDENCE))
                    .chain(pack.evidence.into_iter().map(|e| {
                        let title = e.title.as_deref().unwrap_or("-");
                        ReadItem::new(
                            format!(
                                "evidence event:{} source:{} ts:{} title:{} :: {}",
                                e.event_id, e.source, e.ts, title, e.excerpt
                            ),
                            EVENT_CONFIDENCE,
                        )
                    }))
                    .collect()
            }

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

            // Onboarding / first-run state (issue #6) is owned by the desktop layer's app-settings
            // (app_data/onboarding.json), not the daemon DB, so this DB-backed face has no row to
            // supply — an empty result is the honest answer here rather than a fabricated one. The
            // tool exists on the shared surface so the contract is symmetric (invariant 6); serving
            // its live value is deferred to a shared-store follow-up.
            Tool::DeviceOnboardingGet => Vec::new(),

            // Structured reads (visual recall, lessons.list, memory.get_wrap) use
            // [`Self::read_structured`]; the rest are not read tools (write / action) — never
            // routed here.
            Tool::MemoryGetWrap
            | Tool::LessonsList
            | Tool::LessonsSetActive
            | Tool::VisualRecallStatus
            | Tool::VisualRecallSearchFrames
            | Tool::VisualRecallGetFrame
            | Tool::VisualRecallRescanFrame
            | Tool::VisualRecallSetEnabled
            | Tool::VisualRecallDeleteFrame
            | Tool::MemoryAppendNote
            | Tool::StateProposeUpdate
            | Tool::ActionsExecute => Vec::new(),
        }
    }

    fn read_structured(&self, tool: Tool, params: &ReadParams) -> Option<String> {
        Some(match tool {
            Tool::VisualRecallStatus => self.visual_recall_status_json(),
            Tool::VisualRecallSearchFrames => self.visual_recall_search_json(params),
            Tool::VisualRecallGetFrame => self.visual_recall_get_frame_json(params),
            Tool::VisualRecallRescanFrame => self.visual_recall_rescan_json(params),
            Tool::LessonsList => self.lessons_json(),
            Tool::MemoryGetWrap => self.evening_wrap_json(),
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
            // Flip a lesson's active switch (L1) — the same effect as the Learned UI toggle.
            Tool::LessonsSetActive => {
                let (id, active) = parse_lesson_active_body(body)?;
                if self.db.set_lesson_active(id, active) {
                    Ok(Some(id))
                } else {
                    Err("lesson not found".to_string())
                }
            }
            Tool::VisualRecallSetEnabled => {
                let enabled = parse_enabled_body(body)?;
                let mut settings = self.load_vr_settings();
                settings.enabled = enabled;
                self.save_vr_settings(&settings)?;
                if !enabled {
                    let removed = self.db.purge_auto_screen_frames()?;
                    if removed > 0 {
                        eprintln!("[visual_recall] disabled via API — purged {removed} auto frame(s)");
                    }
                }
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

/// Parse the `lessons.set_active` body: `{"id": <i64>, "active": <bool>}`.
fn parse_lesson_active_body(body: &str) -> Result<(i64, bool), String> {
    let err = || "expected {\"id\":number,\"active\":bool}".to_string();
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| err())?;
    let id = v.get("id").and_then(|x| x.as_i64()).ok_or_else(err)?;
    let active = v.get("active").and_then(|x| x.as_bool()).ok_or_else(err)?;
    Ok((id, active))
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

    fn backend_with_settings(db: Db) -> DbBackend {
        let dir = std::env::temp_dir().join(format!("shogun_vr_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        DbBackend::new(db).with_visual_recall_settings_path(dir.join("visual_recall.json"))
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
    fn context_pack_returns_facts_and_cited_evidence() {
        // seed: one commitment (fact supply) + one captured event matching the query (evidence).
        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let (e, _) = db
            .capture(&shogun_memory::event_log::NewEvent {
                ts: 1,
                source: "gmail",
                kind: "message",
                app_bundle_id: None,
                window_title: Some("Vendor renewal"),
                content: "the vendor renewal was settled at 12k for the year",
                content_hash: "h1",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            })
            .unwrap();
        db.insert_commitment(
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "confirm the vendor contract",
                due_at: Some(50),
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[Provenance::new(e)],
        )
        .unwrap();
        let backend = DbBackend::new(db);

        let items = backend.read(
            Tool::MemoryGetContextPack,
            &ReadParams {
                id: None,
                query: Some("vendor renewal".into()),
                from_ms: None,
                to_ms: None,
            },
        );
        // a fact line and an evidence line, the latter citing its event id + source (provenance).
        assert!(items.iter().any(|i| i.label.starts_with("fact: ")), "items: {items:?}");
        let evidence = items
            .iter()
            .find(|i| i.label.starts_with("evidence "))
            .unwrap_or_else(|| panic!("no evidence line: {items:?}"));
        assert!(evidence.label.contains(&format!("event:{e}")));
        assert!(evidence.label.contains("source:gmail"));
        assert!(evidence.label.contains("12k"));

        // no query → empty (the pack is per-question; there is no "pack of everything").
        assert!(backend.read(Tool::MemoryGetContextPack, &params()).is_empty());
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
    fn visual_recall_status_and_set_enabled() {
        let db = Db::open_in_memory(Arc::new(|| 5_000)).unwrap();
        let backend = backend_with_settings(db);
        let status = backend.read_structured(Tool::VisualRecallStatus, &params()).expect("status");
        assert!(status.contains("\"enabled\":false"));
        assert!(backend.write(Tool::VisualRecallSetEnabled, r#"{"enabled":true}"#).is_ok());
        let status2 = backend.read_structured(Tool::VisualRecallStatus, &params()).expect("status");
        assert!(status2.contains("\"enabled\":true"));
    }

    /// Seed a distilled lesson via the real feedback → distill → upsert path. The draft bodies
    /// carry an unmistakable secret marker so tests can assert it never reaches the API.
    fn seed_lesson(db: &Db) -> i64 {
        use shogun_memory::lessons::{distill, FeedbackKind, LessonScope, NewFeedback};
        const SECRET: &str = "SECRET_FEEDBACK_BODY_q1";
        for i in 0..3 {
            let before = format!(
                "Hi team,\nA longer draft body about the {SECRET} quarterly numbers, line {i}.\nMore detail follows in the tracker.\nBest, Taro"
            );
            let after = format!(
                "Hi team,\nA longer draft body about the {SECRET} quarterly numbers, line {i}.\nMore detail follows in the tracker."
            );
            db.record_feedback(
                FeedbackKind::EditBeforeApprove,
                LessonScope::App,
                &NewFeedback {
                    ts_ms: i,
                    action_kind: Some("draft_reply"),
                    scope_ref: Some("com.apple.Mail"),
                    before_text: Some(&before),
                    after_text: Some(&after),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let candidates = distill(&db.feedback_after(0));
        assert_eq!(candidates.len(), 1);
        db.upsert_lesson(&candidates[0], 100).unwrap()
    }

    #[test]
    fn get_wrap_is_structured_and_carries_the_wrap_shape() {
        // Issue #10 (invariant 6): the API face serves the same evening_wrap aggregation the
        // card draws. Shape only here — the aggregation itself is pinned by the daemon's test.
        let db = Db::open_in_memory(Arc::new(|| 72_000_000)).unwrap();
        let backend = DbBackend::new(db);
        let json = backend.read_structured(Tool::MemoryGetWrap, &params()).expect("structured");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["date"].as_str().is_some_and(|d| d.len() == 10), "local date key");
        assert_eq!(v["outcome"]["commitments_done"], 0);
        assert!(v["still_open"].as_array().is_some());
        assert!(v["tomorrow_calendar"].as_array().is_some());
        assert!(v["loose_ends"].as_array().is_some());
        // And the plain-read face has nothing for it (structured-only tool).
        assert!(backend.read(Tool::MemoryGetWrap, &params()).is_empty());
    }

    #[test]
    fn lessons_list_is_structured_and_never_exposes_feedback_text() {
        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let id = seed_lesson(&db);
        let backend = DbBackend::new(db);

        let json = backend.read_structured(Tool::LessonsList, &params()).expect("structured");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let row = &v["lessons"][0];
        assert_eq!(row["id"], id);
        assert_eq!(row["kind"], "style");
        assert_eq!(row["scope"], "app");
        assert_eq!(row["scope_ref"], "com.apple.Mail");
        assert_eq!(row["evidence_count"], 3);
        assert_eq!(row["active"], true);
        assert!(row["instruction"].as_str().unwrap().contains("Best, Taro"));
        assert!(row["confidence"].as_f64().unwrap() >= 0.5);
        // The invariant: feedback_events text never leaves the DB through the API.
        assert!(!json.contains("SECRET_FEEDBACK_BODY_q1"), "feedback text leaked: {json}");
        assert!(!json.contains("quarterly numbers"), "feedback text leaked: {json}");
    }

    #[test]
    fn lessons_set_active_flips_the_switch_and_rejects_bad_input() {
        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let id = seed_lesson(&db);
        let backend = DbBackend::new(db.clone());

        // off…
        assert_eq!(
            backend.write(Tool::LessonsSetActive, &format!(r#"{{"id":{id},"active":false}}"#)),
            Ok(Some(id))
        );
        assert!(!db.lessons_all()[0].active);
        // …and back on
        assert_eq!(
            backend.write(Tool::LessonsSetActive, &format!(r#"{{"id":{id},"active":true}}"#)),
            Ok(Some(id))
        );
        assert!(db.lessons_all()[0].active);
        // unknown id and malformed bodies error, and lessons.list is not a plain read
        assert!(backend.write(Tool::LessonsSetActive, r#"{"id":9999,"active":false}"#).is_err());
        assert!(backend.write(Tool::LessonsSetActive, "not json").is_err());
        assert!(backend.write(Tool::LessonsSetActive, r#"{"id":1}"#).is_err());
        assert!(backend.read(Tool::LessonsList, &params()).is_empty());
    }

    #[test]
    fn full_stack_lessons_through_rest_respond_with() {
        use shogun_mcp::memory_api::TokenRegistry;
        use shogun_mcp::rest::{respond_with, Method, RestRequest};

        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let id = seed_lesson(&db);
        let backend = DbBackend::new(db.clone());
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let ent = shogun_mcp::entitlement::Entitlements::trial_not_started();
        let req = |method, path: &str, body: Option<&str>| RestRequest {
            method,
            path: path.into(),
            token: Some("t".into()),
            include_low: false,
            query: None,
            body: body.map(str::to_string),
            from_ms: None,
            to_ms: None,
        };

        // GET /v1/lessons lists the lesson (no feedback text — pinned above per-payload)
        let (status, body) = respond_with(&req(Method::Get, "/v1/lessons", None), &tokens, &ent, &backend);
        assert_eq!(status, 200);
        assert!(body.contains("\"tool\":\"lessons.list\""), "{body}");
        assert!(body.contains("Best, Taro"), "{body}");
        assert!(!body.contains("SECRET_FEEDBACK_BODY_q1"), "{body}");

        // POST /v1/lessons/active flips it off through the same face
        let (status, body) = respond_with(
            &req(Method::Post, "/v1/lessons/active", Some(&format!(r#"{{"id":{id},"active":false}}"#))),
            &tokens,
            &ent,
            &backend,
        );
        assert_eq!(status, 202);
        assert!(body.contains("\"level\":\"L1\""), "{body}");
        assert!(!db.lessons_all()[0].active);
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
        let (status, body) = respond_with(&req, &tokens, &shogun_mcp::entitlement::Entitlements::trial_not_started(), &backend);
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
        let (status, body) = respond_with(&req, &tokens, &shogun_mcp::entitlement::Entitlements::trial_not_started(), &backend);
        assert_eq!(status, 200);
        // real DB data rendered through the API layer's confidence-gated JSON
        assert!(body.contains("send the report"), "body: {body}");
        assert!(body.contains("state.commitments.list"));
    }
}
