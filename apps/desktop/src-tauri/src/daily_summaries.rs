//! Daily summaries — delivery state + wrap assembly for the notch cards (Issue #10,
//! docs/daily-summaries-design.md).
//!
//! The judgement itself lives in `shogun_core::daily_delivery` (pure, Linux-tested); this module
//! is the shell seam: it owns `daily_summaries.json` (app-data, non-secret — one flat object
//! holding settings + seen dates), supplies the local clock, and serialises `Db::evening_wrap`
//! into the wire shape the webview draws. Nothing here computes content; the webview computes
//! nothing (invariant 1).

use serde::{Deserialize, Serialize};
use shogun_core::daily_delivery::{SeenState, Settings};
use tauri::Manager;

/// Last global user input (unix ms), stamped by the shell's global event monitors and the
/// panel's own `interact` reports. The delivery judgement runs on a poll, but §2 promises the
/// summary lands at the user's first *activity* — this is how the cue knows someone is actually
/// there, instead of chiming into an empty room at midnight or 17:30 sharp.
pub static LAST_GLOBAL_INPUT_MS: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

pub fn note_global_input(now_ms: i64) {
    LAST_GLOBAL_INPUT_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);
}

/// How recent the last input must be for the user to count as present. Generous on purpose:
/// reading counts as being there, and a missed cue costs more than one played a minute late.
pub const PRESENCE_WINDOW_MS: i64 = 60_000;

/// The persisted file: settings and seen-state flattened into one object, exactly the shape
/// documented in docs/daily-summaries-design.md §2 (`morning_seen_date`, `evening_seen_date`,
/// `evening_hour`, `evening_minute`, `morning_enabled`, `evening_enabled`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stored {
    #[serde(flatten)]
    pub settings: Settings,
    #[serde(flatten)]
    pub seen: SeenState,
}

pub fn state_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("daily_summaries.json"))
}

pub fn load(app: &tauri::AppHandle) -> Stored {
    let Some(path) = state_path(app) else {
        return Stored::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Stored::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Reject out-of-range times before they are persisted: an `evening_hour` of 99 would silently
/// disable the Evening card forever (the `due` comparison could never pass).
pub fn validate(settings: &Settings) -> Result<(), String> {
    if settings.evening_hour > 23 || settings.evening_minute > 59 {
        return Err(format!(
            "evening time out of range: {:02}:{:02}",
            settings.evening_hour, settings.evening_minute
        ));
    }
    Ok(())
}

/// Atomic write (tmp + rename), same as every other settings file — a power cut mid-write must
/// not cost the seen-dates and re-deliver a day's summaries.
pub fn save(app: &tauri::AppHandle, stored: &Stored) -> Result<(), String> {
    let path = state_path(app).ok_or("app data dir unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create state dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(stored).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("write summaries state: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("commit summaries state: {e}"))?;
    Ok(())
}

// ——— wire types (webview draws these verbatim) ———

#[derive(Serialize)]
pub struct SummaryState {
    /// Which card is due right now: `"morning"` / `"evening"` / absent.
    pub due: Option<&'static str>,
    /// Local calendar date (`YYYY-MM-DD`) the judgement was made on — the webview passes it back
    /// to `mark_summary_seen` unchanged so a card opened across midnight marks the day it was
    /// delivered for, not the day the click landed on.
    pub date: String,
    pub settings: Settings,
}

#[derive(Serialize)]
pub struct WrapLine {
    pub text: String,
    /// FR-MB-05: medium-confidence items carry the hedge; the card renders them half-toned.
    pub possibly: bool,
    /// Provenance event id — `open_summary_source` re-opens the data source from this.
    pub provenance_event_id: i64,
    /// Deep-link chip label ("Mail", "Calendar", the captured app's name…), resolved from the
    /// provenance event's source here so the webview draws it verbatim (invariant 1). Absent when
    /// the source has no destination worth offering.
    pub source: Option<String>,
}

#[derive(Serialize)]
pub struct WrapCalendarLine {
    pub time: String,
    pub title: String,
    pub updated: bool,
}

#[derive(Serialize)]
pub struct WrapOutcomeView {
    pub commitments_done: u32,
    pub loops_closed: u32,
    pub actions_decided: u32,
    pub actions_adopted: u32,
}

#[derive(Serialize)]
pub struct WrapView {
    pub outcome: WrapOutcomeView,
    pub still_open: Vec<WrapLine>,
    pub tomorrow_calendar: Vec<WrapCalendarLine>,
    pub tomorrow_commitments: Vec<WrapLine>,
    pub loose_ends: Vec<WrapLine>,
}

/// The Morning card: the persisted nightly brief when one exists, the degraded local assembly
/// when it doesn't (FR-MB-04 — the morning is never an error screen).
#[derive(Serialize)]
pub struct MorningView {
    /// Whether generated prose backs `what_happened` (false = extractive degradation).
    pub generated: bool,
    /// The nightly Charm line (issue #10 M3 fills it; absent until then and on degraded days).
    pub charm_line: Option<String>,
    pub today: Vec<WrapCalendarLine>,
    pub commitments_due: Vec<WrapLine>,
    pub open_loops: Vec<WrapLine>,
    pub what_happened: Vec<String>,
}

#[cfg(target_os = "macos")]
pub mod mac {
    use super::*;
    use shogun_core::daemon::Db;
    use shogun_core::daily_delivery::{due, mark_seen, LocalNow, Which};
    use tauri::Manager;

    fn now_ms(app: &tauri::AppHandle) -> i64 {
        // The Db clock when capture is running (one time source for the whole day window);
        // wall clock when it isn't — the summaries must not go silent just because the memory
        // store failed to open.
        match app.try_state::<Db>() {
            Some(db) => db.now_ms(),
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        }
    }

    /// Local hour/minute for an instant (DST folded in by libc, mirroring
    /// `shogun_core::daemon::local_date_string`).
    fn local_hm(ts_ms: i64) -> (u8, u8) {
        // SAFETY: `tm` is only read after localtime_r reports success by returning non-null.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            let t = (ts_ms / 1000) as libc::time_t;
            if !libc::localtime_r(&t, &mut tm).is_null() {
                return (tm.tm_hour as u8, tm.tm_min as u8);
            }
        }
        let mins_of_day = (ts_ms / 1000).rem_euclid(86_400) / 60;
        ((mins_of_day / 60) as u8, (mins_of_day % 60) as u8)
    }

    fn clock(ts_ms: i64) -> String {
        let (h, m) = local_hm(ts_ms);
        format!("{h:02}:{m:02}")
    }

    /// Chip label for a provenance source. Connector sources get the service's own name (the
    /// user connected it by that name — this is their data's address, not marketing copy);
    /// captured events get the app they were captured in. `None` = no destination worth a chip.
    fn chip_label(source: &str, app_bundle_id: Option<&str>) -> Option<String> {
        Some(match source {
            "gmail" => "Mail".to_string(),
            "gcal" => "Calendar".to_string(),
            "slack" => "Slack".to_string(),
            "notion" => "Notion".to_string(),
            "github" => "GitHub".to_string(),
            "linear" => "Linear".to_string(),
            "meeting" => "Meeting".to_string(),
            "capture" | "screen_ocr" => {
                // "com.apple.Safari" → "Safari". Coarse but honest; a wrong-looking label here
                // beats a lookup table that goes stale.
                app_bundle_id?.rsplit('.').next().filter(|s| !s.is_empty())?.to_string()
            }
            _ => return None,
        })
    }

    fn chip_for(db: Option<&Db>, event_id: i64) -> Option<String> {
        let (source, bundle) = db?.event_source(event_id)?;
        chip_label(&source, bundle.as_deref())
    }

    fn wrap_line(db: Option<&Db>, text: &str, possibly: bool, event_id: i64) -> WrapLine {
        WrapLine {
            text: text.to_string(),
            possibly,
            provenance_event_id: event_id,
            source: chip_for(db, event_id),
        }
    }

    fn brief_item(db: Option<&Db>, i: &shogun_fusion::brief::BriefItem) -> WrapLine {
        wrap_line(db, &i.text, i.possibly, i.provenance_event_id)
    }

    /// One `SummaryReady` cue per delivered card: the judgement fires on every poll, the sound
    /// must not. Process-local on purpose — a relaunch may re-cue an unseen card, which is the
    /// lesser wrong (silence would mean a crash eats the day's only signal).
    static CUED: std::sync::Mutex<Option<(String, &'static str)>> = std::sync::Mutex::new(None);

    /// The delivery judgement, made at the moment of the call (the webview calls this on
    /// activity/paint — the call itself is the "user is here" signal, §2).
    #[tauri::command]
    pub fn summary_state(app: tauri::AppHandle) -> SummaryState {
        let stored = load(&app);
        let now = now_ms(&app);
        let date = shogun_core::daemon::local_date_string(now);
        let (hour, minute) = local_hm(now);
        let which = due(
            LocalNow { date: &date, hour, minute },
            &stored.settings,
            &stored.seen,
        );
        let due_str = which.map(|w| match w {
            Which::Morning => "morning",
            Which::Evening => "evening",
        });
        if let Some(w) = due_str {
            // Cue only while the user is demonstrably present (recent global input) — otherwise
            // leave CUED unset so the chime happens when they actually arrive, not into an empty
            // room the second the threshold passes.
            let last_input = LAST_GLOBAL_INPUT_MS.load(std::sync::atomic::Ordering::Relaxed);
            let present = last_input > 0 && now.saturating_sub(last_input) <= PRESENCE_WINDOW_MS;
            if present {
                if let Ok(mut cued) = CUED.lock() {
                    if cued.as_ref().map(|(d, c)| (d.as_str(), *c)) != Some((date.as_str(), w)) {
                        *cued = Some((date.clone(), w));
                        crate::sound::mac::play(shogun_core::sound::Cue::SummaryReady);
                    }
                }
            }
        }
        SummaryState { due: due_str, date, settings: stored.settings }
    }

    /// The card was opened: advance that side's seen-date so it doesn't re-deliver today.
    #[tauri::command]
    pub fn mark_summary_seen(which: String, date: String, app: tauri::AppHandle) -> Result<(), String> {
        let which = match which.as_str() {
            "morning" => Which::Morning,
            "evening" => Which::Evening,
            other => return Err(format!("unknown summary kind: {other}")),
        };
        let mut stored = load(&app);
        mark_seen(&mut stored.seen, which, &date);
        save(&app, &stored)
    }

    #[tauri::command]
    pub fn get_daily_summary_settings(app: tauri::AppHandle) -> Settings {
        load(&app).settings
    }

    /// Persist new settings, keeping the seen-dates untouched — moving the evening time must not
    /// clear today's "already delivered" (the pure logic's no-redeliver rule depends on it).
    /// Returns what was actually stored (same contract as the sound settings writes).
    #[tauri::command]
    pub fn set_daily_summary_settings(
        settings: Settings,
        app: tauri::AppHandle,
    ) -> Result<Settings, String> {
        validate(&settings)?;
        let mut stored = load(&app);
        stored.settings = settings;
        save(&app, &stored)?;
        Ok(stored.settings)
    }

    /// Assemble the Evening card content: `Db::evening_wrap` over today's local window —
    /// deterministic aggregation only, no LLM call, no egress (§6.17).
    #[tauri::command]
    pub fn evening_wrap(app: tauri::AppHandle) -> Result<WrapView, String> {
        let Some(db) = app.try_state::<Db>() else {
            return Err("Capture isn't running — the memory store couldn't be opened, so there's \
                        no day to sum up yet."
                .to_string());
        };
        let now = db.now_ms();
        let (day_start, tomorrow_end) = shogun_core::daemon::local_wrap_window(now);
        // Calendar lines flow in from the connector lane; until that plumbing exists the section
        // comes back empty rather than invented (same honesty rule as fullui::today).
        let wrap = db.evening_wrap(Vec::new(), day_start, now, tomorrow_end);
        let d = Some(&*db);
        Ok(WrapView {
            outcome: WrapOutcomeView {
                commitments_done: wrap.outcome.commitments_done,
                loops_closed: wrap.outcome.loops_closed,
                actions_decided: wrap.outcome.actions_decided,
                actions_adopted: wrap.outcome.actions_adopted,
            },
            still_open: wrap.still_open.iter().map(|i| brief_item(d, i)).collect(),
            tomorrow_calendar: wrap
                .tomorrow_calendar
                .iter()
                .map(|c| WrapCalendarLine {
                    time: clock(c.start_ms),
                    title: c.title.clone(),
                    updated: c.updated,
                })
                .collect(),
            tomorrow_commitments: wrap.tomorrow_commitments.iter().map(|i| brief_item(d, i)).collect(),
            loose_ends: wrap.loose_ends.iter().map(|i| brief_item(d, i)).collect(),
        })
    }

    /// Assemble the Morning card: the persisted nightly brief for today when one exists
    /// (a read — immediate and offline-stable), else the degraded live assembly (FR-MB-04).
    #[tauri::command]
    pub fn morning_card(app: tauri::AppHandle) -> Result<MorningView, String> {
        let Some(db) = app.try_state::<Db>() else {
            return Err("Capture isn't running — the memory store couldn't be opened, so there's \
                        nothing to show yet."
                .to_string());
        };
        let now = db.now_ms();
        let date = shogun_core::daemon::local_date_string(now);
        let d = Some(&*db);

        if let Some(row) = db.brief_for(&date) {
            if let Ok(p) = serde_json::from_str::<shogun_memory::briefs::BriefPayload>(&row.payload)
            {
                let line = |l: &shogun_memory::briefs::BriefLine| {
                    wrap_line(d, &l.text, l.possibly, l.provenance_event_id)
                };
                return Ok(MorningView {
                    generated: row.generated,
                    charm_line: p.charm_line.clone(),
                    today: p
                        .today
                        .iter()
                        .map(|s| WrapCalendarLine {
                            time: clock(s.start_ms),
                            title: s.title.clone(),
                            updated: s.updated,
                        })
                        .collect(),
                    commitments_due: p.commitments_due.iter().map(line).collect(),
                    open_loops: p.open_loops.iter().map(line).collect(),
                    what_happened: p.what_happened.clone(),
                });
            }
            // An unreadable payload falls through to the degraded assembly — the morning must
            // never be an error screen (FR-MB-04).
        }
        let brief = db.local_morning_brief(Vec::new(), now);
        Ok(MorningView {
            generated: false,
            charm_line: None,
            today: Vec::new(),
            commitments_due: brief.commitments_due.iter().map(|i| brief_item(d, i)).collect(),
            open_loops: brief.open_loops.iter().map(|i| brief_item(d, i)).collect(),
            what_happened: Vec::new(),
        })
    }

    /// Re-open a summary line's data source (the card's deep-link chip, issue #10): a captured
    /// event re-opens the app it was captured in; a connector event opens the service. Metadata
    /// only — nothing about the event's content leaves this process (opening a fixed URL carries
    /// no user data).
    #[tauri::command]
    pub fn open_summary_source(event_id: i64, app: tauri::AppHandle) -> Result<(), String> {
        let Some(db) = app.try_state::<Db>() else {
            return Err("memory store unavailable".to_string());
        };
        let Some((source, bundle)) = db.event_source(event_id) else {
            return Err("no provenance for this line".to_string());
        };
        match source.as_str() {
            "capture" | "screen_ocr" => {
                let bundle = bundle.ok_or("the captured event has no app to open")?;
                crate::notch_exec::mac::open_app(&bundle)
            }
            // Fixed service fronts — never a per-item URL (events don't carry one yet; the
            // design doc's "URL if provenance has one" upgrades this when they do).
            "gmail" => open_in_browser("https://mail.google.com"),
            "gcal" => open_in_browser("https://calendar.google.com"),
            "slack" => crate::notch_exec::mac::open_app("com.tinyspeck.slackmacgap")
                .or_else(|_| open_in_browser("https://app.slack.com")),
            "notion" => crate::notch_exec::mac::open_app("notion.id")
                .or_else(|_| open_in_browser("https://www.notion.so")),
            "github" => open_in_browser("https://github.com"),
            "linear" => open_in_browser("https://linear.app"),
            other => Err(format!("no destination for source {other}")),
        }
    }

    /// Same guarded `open` as billing's: https-only, fixed URLs from the match above.
    fn open_in_browser(url: &str) -> Result<(), String> {
        if !url.starts_with("https://") {
            return Err("refusing to open a non-https URL".into());
        }
        std::process::Command::new("open")
            .arg(url)
            .status()
            .map_err(|e| format!("open failed: {e}"))?;
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub mod mac {
    use super::*;

    #[tauri::command]
    pub fn summary_state(app: tauri::AppHandle) -> SummaryState {
        SummaryState { due: None, date: String::new(), settings: load(&app).settings }
    }

    #[tauri::command]
    pub fn mark_summary_seen(
        _which: String,
        _date: String,
        _app: tauri::AppHandle,
    ) -> Result<(), String> {
        Ok(())
    }

    #[tauri::command]
    pub fn get_daily_summary_settings(app: tauri::AppHandle) -> Settings {
        load(&app).settings
    }

    #[tauri::command]
    pub fn set_daily_summary_settings(
        settings: Settings,
        app: tauri::AppHandle,
    ) -> Result<Settings, String> {
        validate(&settings)?;
        let mut stored = load(&app);
        stored.settings = settings;
        save(&app, &stored)?;
        Ok(stored.settings)
    }

    #[tauri::command]
    pub fn evening_wrap(_app: tauri::AppHandle) -> Result<WrapView, String> {
        Err("macOS only".to_string())
    }

    #[tauri::command]
    pub fn morning_card(_app: tauri::AppHandle) -> Result<MorningView, String> {
        Err("macOS only".to_string())
    }

    #[tauri::command]
    pub fn open_summary_source(_event_id: i64, _app: tauri::AppHandle) -> Result<(), String> {
        Err("macOS only".to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stored_file_is_the_documented_flat_shape() {
        let json = serde_json::to_string(&Stored::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for key in [
            "morning_enabled",
            "evening_enabled",
            "evening_hour",
            "evening_minute",
            "morning_seen_date",
            "evening_seen_date",
        ] {
            assert!(v.get(key).is_some(), "missing flat key {key}");
        }
    }

    #[test]
    fn missing_and_partial_files_take_defaults() {
        let empty: Stored = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Stored::default());
        let partial: Stored =
            serde_json::from_str(r#"{"evening_hour":19,"morning_seen_date":"2026-08-15"}"#).unwrap();
        assert_eq!(partial.settings.evening_hour, 19);
        assert_eq!(partial.settings.evening_minute, 30);
        assert_eq!(partial.seen.morning_seen_date.as_deref(), Some("2026-08-15"));
        assert!(partial.settings.morning_enabled);
    }

    #[test]
    fn out_of_range_evening_times_are_rejected() {
        let mut s = Settings::default();
        assert!(validate(&s).is_ok());
        s.evening_hour = 24;
        assert!(validate(&s).is_err());
        s.evening_hour = 23;
        s.evening_minute = 60;
        assert!(validate(&s).is_err());
    }

    #[test]
    fn roundtrip_preserves_seen_dates() {
        let mut s = Stored::default();
        s.seen.evening_seen_date = Some("2026-08-15".into());
        s.settings.evening_hour = 18;
        let back: Stored =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
