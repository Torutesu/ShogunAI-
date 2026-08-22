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
use shogun_mcp::memory_api_settings::{
    load_settings as load_memory_api_settings, save_settings as save_memory_api_settings,
    validate_profile, Settings as MemoryApiSettings,
};
use shogun_mcp::voice_dictionary_api::{VoiceDictionaryOperation, VoiceDictionaryResult};

use crate::capture::visual_recall::{
    load_settings, save_settings, RetentionPolicy, Settings, DAY_MS,
};
use crate::daemon::{local_day_bounds, Db};

/// Max search hits returned by `memory.search` over the API.
const SEARCH_LIMIT: usize = 20;
/// Max frame hits for `visual_recall.search_frames`.
const FRAME_SEARCH_LIMIT: usize = 20;
/// OCR excerpt length for frame search / status previews.
const FRAME_EXCERPT_CHARS: usize = 200;
/// Recent durable activity included in query-free `memory.get_context` snapshots.
const CONTEXT_ACTIVITY_LIMIT: usize = 8;
/// Per-activity text cap keeps one captured window from consuming the whole snapshot.
const CONTEXT_ACTIVITY_EXCERPT_CHARS: usize = 320;
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

    /// `memory_api.json`: additional explicit opt-in and local profile data.
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

    /// Enforce age expiry on every API face, including processes without the desktop poller.
    fn enforce_vr_retention(&self) -> Result<i64, String> {
        let retention_ms = self
            .load_vr_settings()
            .retention
            .retain_ms()
            .unwrap_or(3 * DAY_MS);
        self.db.purge_screen_frames(retention_ms)?;
        Ok(retention_ms)
    }

    fn load_memory_api_settings(&self) -> MemoryApiSettings {
        self.memory_api_settings_path()
            .map(load_memory_api_settings)
            .unwrap_or_default()
    }

    fn save_memory_api_settings(&self, settings: &MemoryApiSettings) -> Result<(), String> {
        let Some(path) = self.memory_api_settings_path() else {
            return Err("memory API settings path not configured".into());
        };
        save_memory_api_settings(path, settings)
    }

    fn whoami_json(&self) -> String {
        let profile = self.load_memory_api_settings().profile;
        let people: Vec<_> = self
            .db
            .people()
            .into_iter()
            .map(|row| row.display_name)
            .collect();
        let projects: Vec<_> = self.db.projects().into_iter().map(|row| row.name).collect();
        let commitments: Vec<_> = self
            .db
            .commitments_due(self.db.now_ms())
            .into_iter()
            .map(|row| row.description)
            .collect();
        let open_loops: Vec<_> = self
            .db
            .open_loops()
            .into_iter()
            .map(|row| row.description)
            .collect();
        json!({
            "profile": { "display_name": profile.display_name, "role": profile.role, "prefs": profile.prefs },
            "work": {
                "people": { "count": people.len(), "names": people },
                "projects": { "count": projects.len(), "names": projects },
                "commitments": { "count": commitments.len(), "names": commitments },
                "open_loops": { "count": open_loops.len(), "names": open_loops },
            }
        }).to_string()
    }

    fn apply_profile_set(&self, body: &str) -> Result<(), String> {
        let value: serde_json::Value =
            serde_json::from_str(body).map_err(|_| "expected profile JSON object".to_string())?;
        let patch = value.get("profile").unwrap_or(&value);
        let Some(object) = patch.as_object() else {
            return Err("expected profile JSON object".into());
        };
        let mut settings = self.load_memory_api_settings();
        let field = |key: &str| -> Result<Option<String>, String> {
            match object.get(key) {
                Some(value) => value
                    .as_str()
                    .map(|text| Some(text.trim().to_string()))
                    .ok_or_else(|| format!("profile.{key} must be a string")),
                None => Ok(None),
            }
        };
        if let Some(value) = field("display_name")? {
            settings.profile.display_name = value;
        }
        if let Some(value) = field("role")? {
            settings.profile.role = value;
        }
        if let Some(value) = field("prefs")? {
            settings.profile.prefs = value;
        }
        validate_profile(&settings.profile)?;
        self.save_memory_api_settings(&settings)
    }

    /// DB-derived context for local agents. Live AX Notch cache is not available to standalone
    /// Memory API / MCP callers.
    fn get_context_items(&self) -> Vec<ReadItem> {
        let mut items = vec![ReadItem::new(
            "note: live AX Notch context cache is not available to standalone Memory API / MCP; this snapshot is DB-derived only",
            EVENT_CONFIDENCE,
        )];

        items.extend(self.db.inline_memory(12).into_iter().map(|fact| {
            // `inline_memory` already applies the state confidence gate and prefixes medium
            // confidence facts with `possibly:`.
            ReadItem::new(format!("fact: {fact}"), EVENT_CONFIDENCE)
        }));
        items.extend(
            self.db
                .recent_user_notes(8)
                .into_iter()
                .map(|note| ReadItem::new(format!("note: {note}"), EVENT_CONFIDENCE)),
        );
        items.extend(
            self.db
                .recent_context_previews(CONTEXT_ACTIVITY_LIMIT, CONTEXT_ACTIVITY_EXCERPT_CHARS)
                .into_iter()
                .map(|(source, event)| {
                    let title = event.window_title.as_deref().unwrap_or("-");
                    let app = event.app_bundle_id.as_deref().unwrap_or("-");
                    ReadItem::new(
                        format!(
                            "recent activity event:{} source:{} ts:{} title:{} app:{} :: {}",
                            event.id, source, event.ts, title, app, event.excerpt
                        ),
                        EVENT_CONFIDENCE,
                    )
                }),
        );
        items
    }

    fn visual_recall_status_json(&self) -> String {
        let settings = self.load_vr_settings();
        let _ = self.enforce_vr_retention();
        let now = self.db.now_ms();
        let retention_ms = settings.retention.retain_ms().unwrap_or(3 * DAY_MS);
        let frame_stats = self.db.screen_frame_stats();
        let frames_24h = self
            .db
            .screen_frames_count_in_range(now.saturating_sub(DAY_MS), now);
        let frames_retained = self
            .db
            .screen_frames_count_in_range(now.saturating_sub(retention_ms), now);
        let estimated_daily_bytes = (frames_24h >= 2).then(|| {
            self.db
                .screen_frame_bytes_in_range(now.saturating_sub(DAY_MS), now)
        });
        let projected_retention_bytes = estimated_daily_bytes
            .and_then(|bytes| bytes.checked_mul(i64::from(settings.retention.days())));
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
            "enabled": settings.enabled,
            "retention_days": settings.retention.days(),
            "events_24h": self.db.screen_ocr_count_24h(),
            "frames_count": frame_stats.count,
            "frames_24h": frames_24h,
            "frames_retained": frames_retained,
            "frames_oldest_ms": frame_stats.oldest_ms,
            "frames_bytes": frame_stats.total_bytes,
            "estimated_daily_bytes": estimated_daily_bytes,
            "projected_retention_bytes": projected_retention_bytes,
            "capture_paused_storage": self.db.screen_frame_capture_paused(),
            "capture_storage_limit_bytes": shogun_memory::retention::FRAME_CAPTURE_MAX_BYTES,
            "last_capture": last_capture,
            "recent": recent,
        })
        .to_string()
    }

    fn visual_recall_search_json(&self, params: &ReadParams) -> String {
        let query = params.query.as_deref().unwrap_or("");
        let now = self.db.now_ms();
        let retention_ms = self.enforce_vr_retention().unwrap_or_else(|_| {
            self.load_vr_settings()
                .retention
                .retain_ms()
                .unwrap_or(3 * DAY_MS)
        });
        let retention_start = now.saturating_sub(retention_ms);
        let local_days = local_day_bounds(now);
        let (requested_from, requested_to) = match (params.from_ms, params.to_ms) {
            (Some(f), Some(t)) => (f, t),
            (Some(f), None) => (f, now),
            (None, Some(t)) => (0, t),
            (None, None) => {
                shogun_memory::search::visual_recall_window(query, now, local_days, retention_ms)
            }
        };
        let from_ms = requested_from.max(retention_start);
        let to_ms = requested_to.min(now);
        let hits = self.db.search_screen_frames_window(
            query,
            from_ms,
            to_ms,
            FRAME_SEARCH_LIMIT,
            FRAME_EXCERPT_CHARS,
        );
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
        if self.enforce_vr_retention().is_err() {
            return r#"{"error":"retention_check_failed"}"#.to_string();
        }
        let Some(frame_id) = params.id else {
            return r#"{"error":"missing_frame_id"}"#.to_string();
        };
        let Some(s) = self.db.get_screen_frame_summary(frame_id) else {
            return json!({ "error": "not_found", "frame_id": frame_id }).to_string();
        };
        let needs_rescan = shogun_memory::screen_frames::needs_rescan(&s.ocr_text);
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
        if self.enforce_vr_retention().is_err() {
            return r#"{"error":"retention_check_failed"}"#.to_string();
        }
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
                    return json!({ "error": "event_update_failed", "frame_id": frame_id })
                        .to_string();
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

    /// `lessons.list` (L5, Plan D-5): instruction + bookkeeping only — never `feedback_events` text.
    ///
    /// Default (no generation filters) is the Settings/management list: every row, including
    /// sleeping and person-scoped. Pass `for_generation` or a scope id for standing-prompt lookup
    /// so an Alice email lesson does not leak into unrelated drafts (issue #104).
    fn lessons_json(&self, params: &ReadParams) -> String {
        let scoped = params.for_generation
            || params.app_bundle_id.is_some()
            || params.person_id.is_some()
            || params.project_id.is_some();
        let lessons = if scoped {
            let ctx = crate::daemon::GenerationContext {
                app_bundle_id: params.app_bundle_id.as_deref(),
                person_id: params.person_id.as_deref(),
                project_id: params.project_id.as_deref(),
            };
            self.db.active_lessons(&ctx.lesson_scopes(), 32)
        } else {
            self.db.lessons_all()
        };
        let lessons: Vec<_> = lessons
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
        let wrap = self
            .db
            .evening_wrap(Vec::new(), day_start, now, tomorrow_end);
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
    fn manage_voice_dictionary(
        &self,
        operation: VoiceDictionaryOperation,
    ) -> Result<VoiceDictionaryResult, String> {
        match operation {
            VoiceDictionaryOperation::List => {
                self.db.list_voice_terms().map(VoiceDictionaryResult::Terms)
            }
            VoiceDictionaryOperation::Create(term) => self
                .db
                .create_voice_term(&term)
                .map(VoiceDictionaryResult::Term),
            VoiceDictionaryOperation::Update { id, term } => self
                .db
                .update_voice_term(id, &term)?
                .map(VoiceDictionaryResult::Term)
                .ok_or_else(|| "voice dictionary term not found".to_string()),
            VoiceDictionaryOperation::Delete { id } => self
                .db
                .delete_voice_term(id)
                .map(VoiceDictionaryResult::Deleted),
        }
    }

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
            // DB-derived work context; live AX Notch cache remains RAM-only and unavailable here.
            Tool::MemoryGetContext => self.get_context_items(),

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
                let pack = self
                    .db
                    .assemble_context(query, PACK_HITS, PACK_EXCERPT_CHARS);
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

            Tool::StatePeopleList => self
                .db
                .people()
                .into_iter()
                .map(|p| ReadItem::new(p.display_name, p.confidence))
                .collect(),
            Tool::StatePeopleGet => one(id.and_then(|i| self.db.person(i)), |p| {
                ReadItem::new(p.display_name, p.confidence)
            }),
            Tool::StateProjectsList => self
                .db
                .projects()
                .into_iter()
                .map(|p| ReadItem::new(p.name, p.confidence))
                .collect(),
            Tool::StateProjectsGet => one(id.and_then(|i| self.db.project(i)), |p| {
                ReadItem::new(p.name, p.confidence)
            }),
            // Commitments/open loops reuse the Fusion supply. `now` from the daemon clock so
            // `overdue` is consistent with the rest of the daemon.
            Tool::StateCommitmentsList => self
                .db
                .commitments_due(self.db.now_ms())
                .into_iter()
                .map(|c| ReadItem::new(c.description, c.confidence))
                .collect(),
            Tool::StateCommitmentsGet => one(id.and_then(|i| self.db.commitment(i)), |c| {
                ReadItem::new(c.description, c.confidence)
            }),
            Tool::StateOpenLoopsList => self
                .db
                .open_loops()
                .into_iter()
                .map(|o| ReadItem::new(o.description, o.confidence))
                .collect(),
            Tool::StateOpenLoopsGet => one(id.and_then(|i| self.db.open_loop(i)), |o| {
                ReadItem::new(o.description, o.confidence)
            }),

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
            | Tool::ProfileWhoami
            | Tool::VoiceDictionaryList
            // Meeting microphone settings live in the shared app-data file and are intercepted
            // before the DB backend by the MCP/REST adapter.
            | Tool::MeetingMicrophoneGet
            | Tool::VoiceDictionaryCreate
            | Tool::VoiceDictionaryUpdate
            | Tool::VoiceDictionaryDelete
            | Tool::MeetingMicrophoneSet
            | Tool::LessonsSetActive
            | Tool::VisualRecallStatus
            | Tool::VisualRecallSearchFrames
            | Tool::VisualRecallGetFrame
            | Tool::VisualRecallRescanFrame
            | Tool::VisualRecallSetEnabled
            | Tool::VisualRecallSetRetention
            | Tool::VisualRecallDeleteFrame
            | Tool::MemoryAppendNote
            | Tool::ProfileSet
            | Tool::StateProposeUpdate
            | Tool::ActionsExecute
            | Tool::ActionsStatus => Vec::new(),
        }
    }

    fn read_structured(&self, tool: Tool, params: &ReadParams) -> Option<String> {
        Some(match tool {
            Tool::VisualRecallStatus => self.visual_recall_status_json(),
            Tool::VisualRecallSearchFrames => self.visual_recall_search_json(params),
            Tool::VisualRecallGetFrame => self.visual_recall_get_frame_json(params),
            Tool::VisualRecallRescanFrame => self.visual_recall_rescan_json(params),
            Tool::LessonsList => self.lessons_json(params),
            Tool::MemoryGetWrap => self.evening_wrap_json(),
            Tool::ProfileWhoami => self.whoami_json(),
            Tool::VoiceDictionaryList => return None,
            _ => return None,
        })
    }

    fn write(&self, tool: Tool, body: &str) -> WriteResult {
        match tool {
            Tool::ProfileSet => {
                self.apply_profile_set(body)?;
                Ok(None)
            }
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
                        eprintln!(
                            "[visual_recall] disabled via API — purged {removed} auto frame(s)"
                        );
                    }
                }
                Ok(None)
            }
            Tool::VisualRecallSetRetention => {
                let retention = parse_retention_body(body)?;
                let mut settings = self.load_vr_settings();
                settings.retention = retention;
                self.save_vr_settings(&settings)?;
                self.db.purge_screen_frames(retention.retain_ms()?)?;
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
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "expected frame id".to_string())?;
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
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "expected {\"enabled\":bool}".to_string())?;
    v.get("enabled")
        .and_then(|x| x.as_bool())
        .ok_or_else(|| "expected {\"enabled\":bool}".to_string())
}

fn parse_retention_body(body: &str) -> Result<RetentionPolicy, String> {
    if let Ok(days) = body.trim().parse::<u32>() {
        return RetentionPolicy::try_days(days);
    }
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| "expected retention days between 1 and 3650".to_string())?;
    let candidates = [
        value.get("days"),
        value.get("retention_days"),
        value
            .get("retention")
            .and_then(|retention| retention.get("days")),
    ];
    let mut parsed = Vec::new();
    for candidate in candidates.into_iter().flatten() {
        let days = candidate
            .as_u64()
            .and_then(|days| u32::try_from(days).ok())
            .ok_or_else(|| "expected retention days between 1 and 3650".to_string())?;
        parsed.push(days);
    }
    let Some(&days) = parsed.first() else {
        return Err("expected retention days between 1 and 3650".to_string());
    };
    if parsed.iter().any(|candidate| *candidate != days) {
        return Err("conflicting retention day values".to_string());
    }
    RetentionPolicy::try_days(days)
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
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "shogun_vr_test_{}_{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&dir);
        DbBackend::new(db).with_visual_recall_settings_path(dir.join("visual_recall.json"))
    }

    fn params() -> ReadParams {
        ReadParams::default()
    }
    fn get(id: i64) -> ReadParams {
        ReadParams {
            id: Some(id),
            ..ReadParams::default()
        }
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
            .insert_person(
                &NewPerson {
                    display_name: "Alice",
                    confidence: 0.85,
                    now: 1,
                    ..Default::default()
                },
                &[Provenance::new(e)],
            )
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
                ..ReadParams::default()
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
                ..ReadParams::default()
            },
        );
        // a fact line and an evidence line, the latter citing its event id + source (provenance).
        assert!(
            items.iter().any(|i| i.label.starts_with("fact: ")),
            "items: {items:?}"
        );
        let evidence = items
            .iter()
            .find(|i| i.label.starts_with("evidence "))
            .unwrap_or_else(|| panic!("no evidence line: {items:?}"));
        assert!(evidence.label.contains(&format!("event:{e}")));
        assert!(evidence.label.contains("source:gmail"));
        assert!(evidence.label.contains("12k"));

        // no query → empty (the pack is per-question; there is no "pack of everything").
        assert!(backend
            .read(Tool::MemoryGetContextPack, &params())
            .is_empty());
    }

    #[test]
    fn get_context_includes_state_and_user_notes() {
        let db = seed();
        let backend = DbBackend::new(db.clone());
        assert!(backend
            .write(Tool::MemoryAppendNote, "remember the launch checklist")
            .is_ok());

        let items = backend.read(Tool::MemoryGetContext, &params());

        assert!(items
            .iter()
            .any(|item| item.label == "fact: you committed: send the report"));
        assert!(items
            .iter()
            .any(|item| item.label == "note: remember the launch checklist"));
        assert!(items.iter().any(|item| item.label.contains("AX Notch")));
    }

    #[test]
    fn get_context_recent_activity_carries_event_provenance() {
        let db = Db::open_in_memory(Arc::new(|| 2)).unwrap();
        let (event_id, _) = db
            .capture(&shogun_memory::event_log::NewEvent {
                ts: 2,
                source: "gmail",
                kind: "email",
                app_bundle_id: Some("com.example.mail"),
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
            item.label
                .contains(&format!("event:{event_id} source:gmail ts:2"))
                && item.label.contains("title:Roadmap app:com.example.mail")
                && item.label.contains("three open decisions")
        }));
    }

    #[test]
    fn get_context_recent_activity_is_bounded_deduplicated_and_allowlisted() {
        let db = Db::open_in_memory(Arc::new(|| 20)).unwrap();
        for index in 0..10 {
            let content = format!("unique recent activity {index}");
            let hash = format!("recent-{index}");
            db.capture(&shogun_memory::event_log::NewEvent {
                ts: index,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: &content,
                content_hash: &hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            })
            .unwrap();
        }
        db.capture(&shogun_memory::event_log::NewEvent {
            ts: 19,
            source: "screen_ocr",
            kind: "text",
            app_bundle_id: None,
            window_title: None,
            content: "unique recent activity 9",
            content_hash: "duplicate-across-source",
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
        db.capture(&shogun_memory::event_log::NewEvent {
            ts: 20,
            source: "private_internal",
            kind: "text",
            app_bundle_id: None,
            window_title: None,
            content: "must stay outside context previews",
            content_hash: "not-allowlisted",
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        })
        .unwrap();
        let backend = DbBackend::new(db);

        let activity: Vec<_> = backend
            .read(Tool::MemoryGetContext, &params())
            .into_iter()
            .filter(|item| item.label.starts_with("recent activity "))
            .collect();

        assert_eq!(activity.len(), CONTEXT_ACTIVITY_LIMIT);
        assert_eq!(
            activity
                .iter()
                .filter(|item| item.label.contains("unique recent activity 9"))
                .count(),
            1
        );
        assert!(!activity
            .iter()
            .any(|item| item.label.contains("private_internal")));
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
                ..ReadParams::default()
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
        let status = backend
            .read_structured(Tool::VisualRecallStatus, &params())
            .expect("status");
        assert!(status.contains("\"enabled\":false"));
        assert!(backend
            .write(Tool::VisualRecallSetEnabled, r#"{"enabled":true}"#)
            .is_ok());
        let status2 = backend
            .read_structured(Tool::VisualRecallStatus, &params())
            .expect("status");
        assert!(status2.contains("\"enabled\":true"));
    }

    #[test]
    fn retention_write_validates_boundaries_and_purges_immediately_by_age() {
        use std::sync::atomic::{AtomicI64, Ordering};

        let clock = Arc::new(AtomicI64::new(0));
        let clock_for_db = clock.clone();
        let db = Db::open_in_memory(Arc::new(move || clock_for_db.load(Ordering::SeqCst))).unwrap();
        let (old_event, _) = db.capture(&ev("old-frame")).unwrap();
        assert!(db
            .store_screen_frame(old_event, None, None, None, 10, 10, b"old")
            .is_some());

        clock.store(2 * DAY_MS, Ordering::SeqCst);
        let (recent_event, _) = db.capture(&ev("recent-frame")).unwrap();
        assert!(db
            .store_screen_frame(recent_event, None, None, None, 10, 10, b"recent")
            .is_some());
        assert_eq!(db.screen_frame_stats().count, 2);

        let backend = backend_with_settings(db.clone());
        assert!(backend
            .write(Tool::VisualRecallSetRetention, r#"{"days":1}"#)
            .is_ok());
        assert_eq!(db.screen_frame_stats().count, 1);
        let status = backend
            .read_structured(Tool::VisualRecallStatus, &params())
            .expect("status");
        assert!(status.contains("\"retention_days\":1"));

        for invalid in [r#"{"days":0}"#, r#"{"days":3651}"#, r#"{"days":"forever"}"#] {
            assert!(backend
                .write(Tool::VisualRecallSetRetention, invalid)
                .is_err());
        }
    }

    #[test]
    fn retention_parser_accepts_legacy_and_nested_shapes() {
        assert_eq!(parse_retention_body("7").unwrap().days(), 7);
        assert_eq!(
            parse_retention_body(r#"{"retention_days":14}"#)
                .unwrap()
                .days(),
            14
        );
        assert_eq!(
            parse_retention_body(r#"{"retention":{"days":30}}"#)
                .unwrap()
                .days(),
            30
        );
        assert_eq!(
            parse_retention_body(r#"{"days":7,"retention_days":7}"#)
                .unwrap()
                .days(),
            7
        );
        assert!(parse_retention_body(r#"{"days":7,"retention_days":14}"#).is_err());
    }

    #[test]
    fn expired_frame_id_is_unavailable_without_desktop_poller() {
        use std::sync::atomic::{AtomicI64, Ordering};

        let clock = Arc::new(AtomicI64::new(0));
        let clock_for_db = clock.clone();
        let db = Db::open_in_memory(Arc::new(move || clock_for_db.load(Ordering::SeqCst))).unwrap();
        let (event_id, _) = db.capture(&ev("lazy-expiry")).unwrap();
        let frame_id = db
            .store_screen_frame(event_id, None, None, None, 10, 10, b"jpeg")
            .expect("stored frame");
        let backend = backend_with_settings(db.clone());

        clock.store(4 * DAY_MS, Ordering::SeqCst);
        let result = backend
            .read_structured(
                Tool::VisualRecallGetFrame,
                &ReadParams {
                    id: Some(frame_id),
                    ..ReadParams::default()
                },
            )
            .expect("structured result");
        assert!(result.contains("not_found"));
        assert_eq!(db.screen_frame_stats().count, 0);
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
        let json = backend
            .read_structured(Tool::MemoryGetWrap, &params())
            .expect("structured");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            v["date"].as_str().is_some_and(|d| d.len() == 10),
            "local date key"
        );
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

        let json = backend
            .read_structured(Tool::LessonsList, &params())
            .expect("structured");
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
        assert!(
            !json.contains("SECRET_FEEDBACK_BODY_q1"),
            "feedback text leaked: {json}"
        );
        assert!(
            !json.contains("quarterly numbers"),
            "feedback text leaked: {json}"
        );
    }

    #[test]
    fn lessons_list_for_generation_drops_person_lessons_until_scoped() {
        use shogun_memory::lessons::{
            FeedbackKind, LessonCandidate, LessonKind, LessonScope, NewFeedback,
        };

        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let seed = |scope: LessonScope, scope_ref: Option<&str>, instruction: &str| {
            let evidence = (0..3)
                .map(|ts_ms| {
                    db.record_feedback(
                        FeedbackKind::EditBeforeApprove,
                        scope,
                        &NewFeedback {
                            ts_ms,
                            scope_ref,
                            action_kind: Some("draft_reply"),
                            ..Default::default()
                        },
                    )
                    .expect("feedback")
                })
                .collect();
            db.upsert_lesson(
                &LessonCandidate {
                    kind: LessonKind::Style,
                    scope,
                    scope_ref: scope_ref.map(str::to_owned),
                    instruction: instruction.to_owned(),
                    evidence,
                },
                100,
            )
            .expect("lesson");
        };
        seed(LessonScope::Global, None, "GLOBAL_LESSON");
        seed(
            LessonScope::Person,
            Some("alice@example.com"),
            "ALICE_ONLY_LESSON",
        );
        let backend = DbBackend::new(db);

        let all = backend
            .read_structured(Tool::LessonsList, &params())
            .expect("structured");
        assert!(all.contains("GLOBAL_LESSON"), "{all}");
        assert!(
            all.contains("ALICE_ONLY_LESSON"),
            "management list keeps person rows: {all}"
        );

        let lookup = backend
            .read_structured(
                Tool::LessonsList,
                &ReadParams {
                    for_generation: true,
                    ..ReadParams::default()
                },
            )
            .expect("structured");
        assert!(lookup.contains("GLOBAL_LESSON"), "{lookup}");
        assert!(
            !lookup.contains("ALICE_ONLY_LESSON"),
            "unscoped generation lookup must not leak person lessons: {lookup}"
        );

        let alice = backend
            .read_structured(
                Tool::LessonsList,
                &ReadParams {
                    person_id: Some("alice@example.com".into()),
                    ..ReadParams::default()
                },
            )
            .expect("structured");
        assert!(alice.contains("GLOBAL_LESSON"), "{alice}");
        assert!(alice.contains("ALICE_ONLY_LESSON"), "{alice}");
    }

    #[test]
    fn lessons_set_active_flips_the_switch_and_rejects_bad_input() {
        let db = Db::open_in_memory(Arc::new(|| 100)).unwrap();
        let id = seed_lesson(&db);
        let backend = DbBackend::new(db.clone());

        // off…
        assert_eq!(
            backend.write(
                Tool::LessonsSetActive,
                &format!(r#"{{"id":{id},"active":false}}"#)
            ),
            Ok(Some(id))
        );
        assert!(!db.lessons_all()[0].active);
        // …and back on
        assert_eq!(
            backend.write(
                Tool::LessonsSetActive,
                &format!(r#"{{"id":{id},"active":true}}"#)
            ),
            Ok(Some(id))
        );
        assert!(db.lessons_all()[0].active);
        // unknown id and malformed bodies error, and lessons.list is not a plain read
        assert!(backend
            .write(Tool::LessonsSetActive, r#"{"id":9999,"active":false}"#)
            .is_err());
        assert!(backend.write(Tool::LessonsSetActive, "not json").is_err());
        assert!(backend
            .write(Tool::LessonsSetActive, r#"{"id":1}"#)
            .is_err());
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
            for_generation: false,
            app_bundle_id: None,
            person_id: None,
            project_id: None,
        };

        // GET /v1/lessons lists the lesson (no feedback text — pinned above per-payload)
        let (status, body) = respond_with(
            &req(Method::Get, "/v1/lessons", None),
            &tokens,
            &ent,
            &backend,
        );
        assert_eq!(status, 200);
        assert!(body.contains("\"tool\":\"lessons.list\""), "{body}");
        assert!(body.contains("Best, Taro"), "{body}");
        assert!(!body.contains("SECRET_FEEDBACK_BODY_q1"), "{body}");

        let (status, lookup) = respond_with(
            &RestRequest {
                for_generation: true,
                ..req(Method::Get, "/v1/lessons", None)
            },
            &tokens,
            &ent,
            &backend,
        );
        assert_eq!(status, 200);
        assert!(
            !lookup.contains("Best, Taro"),
            "unscoped generation lookup must drop app lessons: {lookup}"
        );
        let (status, scoped) = respond_with(
            &RestRequest {
                app_bundle_id: Some("com.apple.Mail".into()),
                ..req(Method::Get, "/v1/lessons", None)
            },
            &tokens,
            &ent,
            &backend,
        );
        assert_eq!(status, 200);
        assert!(scoped.contains("Best, Taro"), "{scoped}");

        // POST /v1/lessons/active flips it off through the same face
        let (status, body) = respond_with(
            &req(
                Method::Post,
                "/v1/lessons/active",
                Some(&format!(r#"{{"id":{id},"active":false}}"#)),
            ),
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
            for_generation: false,
            app_bundle_id: None,
            person_id: None,
            project_id: None,
        };
        let (status, body) = respond_with(
            &req,
            &tokens,
            &shogun_mcp::entitlement::Entitlements::trial_not_started(),
            &backend,
        );
        assert_eq!(status, 202);
        assert!(body.contains("memory.append_note"));
        assert!(body.contains("\"level\":\"L1\""));
        assert!(
            body.contains("\"id\":"),
            "persisted note id missing: {body}"
        );
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
            for_generation: false,
            app_bundle_id: None,
            person_id: None,
            project_id: None,
        };
        let (status, body) = respond_with(
            &req,
            &tokens,
            &shogun_mcp::entitlement::Entitlements::trial_not_started(),
            &backend,
        );
        assert_eq!(status, 200);
        // real DB data rendered through the API layer's confidence-gated JSON
        assert!(body.contains("send the report"), "body: {body}");
        assert!(body.contains("state.commitments.list"));
    }
}
