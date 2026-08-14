//! The Full UI view (spec §D), assembled in the core.
//!
//! CLAUDE.md invariant 1: the data layer lives in Rust. The webview draws what arrives here and
//! computes nothing — so every string the window shows (a duration, a byte count, a freshness
//! label) is formatted on this side.
//!
//! The rule this module holds to: **emit only what is actually measured.** Coverage, blind spots,
//! yield and grounding have no source in the tree yet, so their cards are simply absent rather
//! than sent as zeroes. This pane's entire job is telling the user what SHOGUN can and can't see;
//! a fabricated number here would undermine the one screen that exists to be trusted. Same for the
//! SLO list — the histograms are WP1.4, so it ships empty until they exist.

use serde::Serialize;

// ——— wire types (mirror apps/desktop/src/fullui/types.ts) ———

#[derive(Serialize)]
pub struct FixLink {
    pub label: String,
    pub target: &'static str,
}

#[derive(Serialize)]
pub struct HealthCard {
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
    pub detail: Option<String>,
    pub fix: Option<FixLink>,
}

#[derive(Serialize)]
pub struct ConfidenceMix {
    pub high_pct: u8,
    pub medium_pct: u8,
    pub low_pct: u8,
}

#[derive(Serialize)]
pub struct SloRow {
    pub name: &'static str,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub target: &'static str,
    pub within_target: bool,
}

#[derive(Serialize)]
pub struct HealthView {
    pub cards: Vec<HealthCard>,
    pub mix: Option<ConfidenceMix>,
    pub slo: Vec<SloRow>,
}

#[derive(Serialize)]
pub struct BriefSection {
    pub heading: String,
    pub body: Option<String>,
    pub bullets: Vec<String>,
}

#[derive(Serialize)]
pub struct SuggestedAction {
    pub id: String,
    pub label: String,
    pub locked: bool,
}

#[derive(Serialize)]
pub struct ScheduleItem {
    pub id: String,
    pub time: String,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct TodayView {
    pub generated: bool,
    pub never_run: bool,
    pub sections: Vec<BriefSection>,
    pub actions: Vec<SuggestedAction>,
    pub schedule: Vec<ScheduleItem>,
}

#[derive(Serialize)]
pub struct SourceRow {
    pub id: String,
    pub name: String,
    pub mark: String,
    pub tint: &'static str,
    pub scope: String,
    pub freshness: String,
    pub health: &'static str,
    pub third_party: bool,
}

#[derive(Serialize)]
pub struct ExclusionRow {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub locked: bool,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct SourcesView {
    pub sources: Vec<SourceRow>,
    pub exclusions: Vec<ExclusionRow>,
    pub ai_sessions_on: bool,
}

#[derive(Serialize)]
pub struct StateRow {
    pub id: String,
    pub text: String,
    pub detail: String,
    pub confidence: &'static str,
}

#[derive(Serialize)]
pub struct MergeCandidate {
    pub id: String,
    pub names: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct MemoryView {
    pub commitments: Vec<StateRow>,
    pub merge_candidates: Vec<MergeCandidate>,
}

#[derive(Serialize)]
pub struct RunRow {
    pub id: String,
    pub time: String,
    pub action: String,
    pub level: &'static str,
    pub approved_by: String,
    pub result: &'static str,
    pub egress: Option<String>,
}

#[derive(Serialize)]
pub struct PendingApproval {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub level: &'static str,
}

#[derive(Serialize)]
pub struct NightlyCycle {
    pub finished_at: String,
    pub events_read: i64,
    pub updates: i64,
    pub chunks_sent: i64,
    pub health: &'static str,
}

#[derive(Serialize)]
pub struct ActivityView {
    pub pending: Vec<PendingApproval>,
    pub runs: Vec<RunRow>,
    pub nightly: NightlyCycle,
}

#[derive(Serialize)]
pub struct EgressRow {
    pub id: String,
    pub time: String,
    pub route: &'static str,
    pub purpose: String,
    pub destination: String,
    pub digest: String,
    pub bytes: String,
}

#[derive(Serialize)]
pub struct TraceView {
    pub rows: Vec<EgressRow>,
    pub third_party_count: usize,
}

#[derive(Serialize)]
pub struct FullUiView {
    pub plan: &'static str,
    pub today: TodayView,
    pub health: HealthView,
    pub sources: SourcesView,
    pub memory: MemoryView,
    pub activity: ActivityView,
    pub trace: TraceView,
}

// ——— formatting helpers (kept here so the webview never does unit math) ———

/// A brief line, keeping the medium-confidence hedge the brief assigned it (FR-MB-05). Dropping
/// the marker here would state a guess as fact.
#[cfg(target_os = "macos")]
fn brief_line(i: &shogun_fusion::brief::BriefItem) -> String {
    if i.possibly {
        format!("possibly: {}", i.text)
    } else {
        i.text.clone()
    }
}

/// "3m ago" / "12m ago" / "2h ago", or a dash when a service has never synced.
fn freshness(last_sync_ms: Option<i64>, now_ms: i64) -> String {
    match last_sync_ms {
        None => "never synced".to_string(),
        Some(ts) => {
            let mins = (now_ms - ts).max(0) / 60_000;
            if mins < 1 {
                "just now".to_string()
            } else if mins < 60 {
                format!("{mins}m ago")
            } else {
                format!("{}h ago", mins / 60)
            }
        }
    }
}

fn clock(ts_ms: i64) -> String {
    // Local wall-clock without pulling in a date crate: the core stores unix-ms, and the window
    // only needs hh:mm.
    let secs = ts_ms / 1000;
    let mins_of_day = (secs % 86_400) / 60;
    format!("{:02}:{:02}", mins_of_day / 60, mins_of_day % 60)
}

fn bytes_label(n: i64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// Confidence band, matching the data-model rule that anything below the low bar must be hedged
/// rather than stated (FR-ST-20).
fn band(confidence: f64) -> &'static str {
    if confidence >= 0.8 {
        "high"
    } else if confidence >= 0.5 {
        "medium"
    } else {
        "low"
    }
}

#[cfg(target_os = "macos")]
pub mod mac {
    use super::*;
    use shogun_core::daemon::Db;

    /// Assemble the Full UI view from the core's own state.
    ///
    /// Sections with a real source are filled from it; sections whose source doesn't exist yet
    /// come back empty, which the window renders as an honest "nothing here" rather than a
    /// placeholder. Nothing in this function invents a value.
    /// Connectors and the approval queue are only managed once the connector runtime starts, and
    /// a machine without service credentials never starts one — by design ("not fatal", lib.rs).
    /// Declaring them as `State` parameters would therefore fail this command outright on the
    /// common case and blank the whole window, so they are looked up optionally and their sections
    /// simply come back empty.
    #[tauri::command]
    pub fn full_ui_view(app: tauri::AppHandle) -> Result<FullUiView, String> {
        use tauri::Manager;
        // The shell deliberately keeps running when the memory store can't be opened (lib.rs:
        // "the daemon simply doesn't capture"), so `Db` may legitimately be absent. Declaring it
        // as a `State` parameter turned that survivable condition into a raw Tauri error in the
        // window; say what actually happened instead.
        let Some(db) = app.try_state::<Db>() else {
            return Err(
                "Capture isn't running — the memory store couldn't be opened, so there's \
                        nothing to show yet. Check the app log for the reason."
                    .to_string(),
            );
        };
        let now = db.now_ms();
        let metrics = app.state::<crate::metrics::SloRegister>();
        let connectors = app.try_state::<crate::connectors::mac::ConnectorState>();
        let approvals = app.try_state::<crate::approvals::mac::ApprovalQueueState>();

        Ok(FullUiView {
            // Billing isn't wired yet (§6.12). Reporting "pro" would silently unlock gated UI, so
            // until the licence check exists the window is told it's on trial — the state that
            // shows everything without claiming the user has paid for it.
            plan: "trial",
            today: today(&db, now),
            health: health(&db, &metrics, now),
            sources: sources(connectors.as_ref(), &app, &db, now)?,
            memory: memory(&db),
            activity: activity(&db, approvals.as_ref())?,
            trace: trace(&db, now),
        })
    }

    fn today(db: &Db, now: i64) -> TodayView {
        // The local brief is the degraded shape by definition (calendar + overdue, no generated
        // prose); the full one arrives from the nightly cycle. Calendar lines aren't plumbed into
        // this window yet, so the schedule comes back empty rather than invented.
        let brief = db.local_morning_brief(Vec::new(), now);
        let mut sections = Vec::new();
        if !brief.commitments_due.is_empty() {
            sections.push(BriefSection {
                heading: "Commitments due".to_string(),
                body: None,
                bullets: brief.commitments_due.iter().map(brief_line).collect(),
            });
        }
        if !brief.open_loops.is_empty() {
            sections.push(BriefSection {
                heading: "Open loops".to_string(),
                body: None,
                bullets: brief.open_loops.iter().map(brief_line).collect(),
            });
        }
        TodayView {
            generated: false,
            never_run: sections.is_empty(),
            sections,
            // Fusion drives these in the panel; surfacing them here is a later work package.
            actions: Vec::new(),
            schedule: Vec::new(),
        }
    }

    /// Health over the last 24 hours. Every card here is computed from something the core
    /// actually recorded; a metric without a source stays absent (see the module header).
    fn health(db: &Db, metrics: &crate::metrics::SloRegister, now: i64) -> HealthView {
        const DAY_MS: i64 = 24 * 60 * 60 * 1000;
        let since = now - DAY_MS;
        let mut cards = Vec::new();

        // Coverage — hours with any capture, not hours of wall time. An idle afternoon should
        // read as a gap rather than being averaged away by a busy morning.
        let hours = db.hours_covered(since, now);
        cards.push(HealthCard {
            key: "coverage",
            label: "Coverage",
            value: format!("{hours}h / 24h captured"),
            detail: (hours < 24).then(|| format!("{} hour(s) with nothing recorded.", 24 - hours)),
            fix: Some(FixLink {
                label: "Open capture rules".to_string(),
                target: "settings",
            }),
        });

        // Yield — the funnel from raw events to what is actually being tracked. `state_changes`
        // is the nightly cycle's own count of what it promoted; `tracked` is what survives now.
        let d = crate::dream::mac::status_view(db);
        let events = db.events_count(since, now);
        let tracked = db
            .commitment_rows()
            .iter()
            .filter(|c| c.status != "done" && c.status != "cancelled")
            .count()
            + db.open_loop_rows().len();
        cards.push(HealthCard {
            key: "yield",
            label: "Yield",
            value: format!("{events} → {} → {tracked} tracked", d.state_changes),
            detail: Some("events → candidates → tracked, over 24h".to_string()),
            fix: Some(FixLink {
                label: "Nightly review".to_string(),
                target: "activity",
            }),
        });

        // Grounding — the share of answers that cited a source. Absent until an answer exists,
        // because a rate over zero answers is undefined rather than 0%.
        if let Some(pct) = metrics.grounding_pct() {
            cards.push(HealthCard {
                key: "grounding",
                label: "Grounding",
                value: format!("{pct}% of answers cited a source"),
                detail: Some("This run.".to_string()),
                fix: Some(FixLink {
                    label: "Widen the search window".to_string(),
                    target: "settings",
                }),
            });
        }

        cards.push(HealthCard {
            key: "egress",
            label: "Egress",
            value: format!("{} chunks last cycle", d.chunks_sent),
            detail: Some(if d.batch_lane {
                "Processing chunks only — never raw capture.".to_string()
            } else {
                "Running locally; nothing was sent.".to_string()
            }),
            fix: Some(FixLink {
                label: "Open Traceability".to_string(),
                target: "trace",
            }),
        });

        HealthView {
            cards,
            // Needs the nightly classifier's own confidence tallies; not exposed yet.
            mix: None,
            slo: metrics
                .rows()
                .into_iter()
                .map(|r| SloRow {
                    name: r.name,
                    p50: r.p50,
                    p95: r.p95,
                    target: r.target,
                    within_target: r.within_target,
                })
                .collect(),
        }
    }

    fn sources(
        connectors: Option<&tauri::State<'_, crate::connectors::mac::ConnectorState>>,
        app: &tauri::AppHandle,
        db: &Db,
        now: i64,
    ) -> Result<SourcesView, String> {
        // No connector runtime → no services to report, not an error.
        let statuses = match connectors {
            None => Vec::new(),
            Some(c) => {
                let rt =
                    c.0.lock()
                        .map_err(|_| "runtime lock poisoned".to_string())?;
                rt.statuses(now)
            }
        };
        let sources = statuses
            .into_iter()
            .map(|s| SourceRow {
                id: s.source.to_string(),
                name: display_name(s.source).to_string(),
                mark: display_name(s.source)
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_string(),
                tint: "var(--accent)",
                scope: if s.has_endpoint {
                    "read".to_string()
                } else {
                    "not available yet".to_string()
                },
                freshness: freshness(s.last_sync_ms, now),
                health: match format!("{:?}", s.state).as_str() {
                    x if x.contains("Connected") => "ok",
                    x if x.contains("Error") || x.contains("Expired") => "warn",
                    _ => "down",
                },
                third_party: false,
            })
            .collect::<Vec<_>>();

        let mut all_sources = visual_recall_source_row(db, now);
        all_sources.extend(sources);

        // What SHOGUN refuses to read. Sourced from the live policy rather than restated here —
        // a screen claiming "password managers are excluded" while the policy disagreed would be
        // worse than saying nothing at all.
        let mut exclusions: Vec<ExclusionRow> =
            shogun_core::capture::exclusion::default_categories()
                .into_iter()
                .map(|(title, n)| ExclusionRow {
                    id: title.to_string(),
                    title: title.to_string(),
                    detail: format!("{n} always excluded — this can't be turned off."),
                    locked: true,
                    enabled: true,
                })
                .collect();
        // Anything the user layered on top of the defaults.
        if let Some(policy) = crate::exclusions::mac::shared() {
            if let Ok(p) = policy.lock() {
                let extra = p.user_apps().len();
                if extra > 0 {
                    exclusions.push(ExclusionRow {
                        id: "user".to_string(),
                        title: "Your own exclusions".to_string(),
                        detail: format!("{extra} app(s) from exclusions.json."),
                        locked: false,
                        enabled: true,
                    });
                }
            }
        }

        Ok(SourcesView {
            sources: all_sources,
            exclusions,
            ai_sessions_on: crate::ai_sessions::mac::get_ai_session_import(app.clone()),
        })
    }

    fn visual_recall_source_row(db: &Db, now: i64) -> Vec<SourceRow> {
        let settings = crate::visual_recall::mac::get_visual_recall_settings();
        if !settings.enabled {
            return vec![SourceRow {
                id: "screen_ocr".to_string(),
                name: "Visual recall".to_string(),
                mark: "V".to_string(),
                tint: "var(--accent)",
                scope: "off — opt in from Settings".to_string(),
                freshness: "—".to_string(),
                health: "down",
                third_party: false,
            }];
        }
        let count_24h = db.screen_ocr_count_24h();
        let latest = db.screen_ocr_previews(1, 80).into_iter().next();
        let (freshness, health) = match latest {
            Some(p) => {
                let label = freshness(Some(p.ts), now);
                let scope_hint = p.app_bundle_id.as_deref().unwrap_or("unknown app");
                let detail = format!("{scope_hint} · {} chars", p.content_len);
                (format!("{label} · {detail}"), "ok")
            }
            None => ("waiting for OCR window".to_string(), "warn"),
        };
        vec![SourceRow {
            id: "screen_ocr".to_string(),
            name: "Visual recall".to_string(),
            mark: "V".to_string(),
            tint: "var(--accent)",
            scope: format!("on-device OCR · {count_24h} reads / 24h"),
            freshness,
            health,
            third_party: false,
        }]
    }

    fn memory(db: &Db) -> MemoryView {
        let commitments = db
            .commitment_rows()
            .into_iter()
            .filter(|c| c.status != "done" && c.status != "cancelled")
            .map(|c| StateRow {
                id: c.id.to_string(),
                text: c.description.clone(),
                detail: format!("{:.0}% sure · {}", c.confidence * 100.0, c.status),
                confidence: band(c.confidence),
            })
            .collect();
        MemoryView {
            commitments,
            // Name resolution / merge review is a later work package (spec §D3).
            merge_candidates: Vec::new(),
        }
    }

    fn activity(
        db: &Db,
        approvals: Option<&tauri::State<'_, crate::approvals::mac::ApprovalQueueState>>,
    ) -> Result<ActivityView, String> {
        // No queue yet → nothing is waiting, which is the truth rather than a failure.
        let pending = match approvals {
            None => Vec::new(),
            Some(a) => {
                // The notch's context-cache path (SLO: 300ms on focus switch) shares this mutex, so
                // hold it only long enough to copy the raw previews out — the formatting below
                // (format!/matches!) then runs with the lock already released.
                let raw: Vec<_> = {
                    // Prefer the shared file so MCP-enqueued L3 sends show in the Notch too.
                    if let Some(path) = &a.path {
                        let q = shogun_mcp::approval_store::load_queue(path)
                            .map_err(|e| e.to_string())?;
                        q.pending_ids()
                            .into_iter()
                            .filter_map(|id| q.preview(id).map(|p| (id, p.clone())))
                            .collect()
                    } else {
                        let q = a
                            .queue
                            .lock()
                            .map_err(|_| "approval queue lock poisoned".to_string())?;
                        q.pending_ids()
                            .into_iter()
                            .filter_map(|id| q.preview(id).map(|p| (id, p.clone())))
                            .collect()
                    }
                };
                raw.into_iter()
                    .map(|(id, p)| PendingApproval {
                        id: format!("{id:?}"),
                        title: format!("{} — {}", p.op_type, p.destination),
                        // Anything leaving the device is L3 by definition (invariant 4).
                        detail: if matches!(p.route, shogun_agents::approval::Route::ViaComposio) {
                            "Leaves the device via a third party".to_string()
                        } else {
                            "Leaves the device directly".to_string()
                        },
                        level: "L3",
                    })
                    .collect()
            }
        };

        let d = crate::dream::mac::status_view(db);
        Ok(ActivityView {
            pending,
            // The agent run log (FR-AG-18) isn't persisted yet — an empty list is what the window
            // needs to render "nothing has run", and it must not be faked.
            runs: Vec::new(),
            nightly: NightlyCycle {
                finished_at: if d.last_ended_at > 0 {
                    clock(d.last_ended_at)
                } else {
                    "—".to_string()
                },
                events_read: d.events_processed,
                updates: d.state_changes,
                chunks_sent: d.chunks_sent,
                health: match d.indicator {
                    "normal" => "ok",
                    "amber" => "warn",
                    _ => "down",
                },
            },
        })
    }

    fn trace(db: &Db, now: i64) -> TraceView {
        let rows: Vec<EgressRow> = db
            .trace_rows(&shogun_memory::traceability::Filter::default())
            .into_iter()
            .enumerate()
            .map(|(i, e)| EgressRow {
                id: format!("{i}-{}", e.ts),
                time: clock(e.ts),
                route: if e.third_party {
                    "third_party"
                } else {
                    "direct"
                },
                purpose: e.purpose.clone(),
                destination: e.destination.clone(),
                // Digest only — the body is never logged, which is what makes this screen safe to
                // show at all (invariant 3).
                digest: format!("xxh64:{}", e.chunk_xxh64),
                bytes: bytes_label(e.chunk_bytes),
            })
            .collect();
        let third_party_count = rows.iter().filter(|r| r.route == "third_party").count();
        let _ = now;
        TraceView {
            rows,
            third_party_count,
        }
    }

    fn display_name(source: &str) -> &str {
        match source {
            "gmail" => "Mail",
            "gcal" => "Calendar",
            "gdrive" => "Drive",
            other => other,
        }
    }
}
