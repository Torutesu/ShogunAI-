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
    /// Provenance event id — the deep-link chip resolves its source from this.
    pub provenance_event_id: i64,
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

    fn brief_item(i: &shogun_fusion::brief::BriefItem) -> WrapLine {
        WrapLine {
            text: i.text.clone(),
            possibly: i.possibly,
            provenance_event_id: i.provenance_event_id,
        }
    }

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
        SummaryState {
            due: which.map(|w| match w {
                Which::Morning => "morning",
                Which::Evening => "evening",
            }),
            date,
            settings: stored.settings,
        }
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
    #[tauri::command]
    pub fn set_daily_summary_settings(settings: Settings, app: tauri::AppHandle) -> Result<(), String> {
        validate(&settings)?;
        let mut stored = load(&app);
        stored.settings = settings;
        save(&app, &stored)
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
        Ok(WrapView {
            outcome: WrapOutcomeView {
                commitments_done: wrap.outcome.commitments_done,
                loops_closed: wrap.outcome.loops_closed,
                actions_decided: wrap.outcome.actions_decided,
                actions_adopted: wrap.outcome.actions_adopted,
            },
            still_open: wrap.still_open.iter().map(brief_item).collect(),
            tomorrow_calendar: wrap
                .tomorrow_calendar
                .iter()
                .map(|c| WrapCalendarLine {
                    time: clock(c.start_ms),
                    title: c.title.clone(),
                    updated: c.updated,
                })
                .collect(),
            tomorrow_commitments: wrap.tomorrow_commitments.iter().map(brief_item).collect(),
            loose_ends: wrap.loose_ends.iter().map(brief_item).collect(),
        })
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
    pub fn set_daily_summary_settings(settings: Settings, app: tauri::AppHandle) -> Result<(), String> {
        validate(&settings)?;
        let mut stored = load(&app);
        stored.settings = settings;
        save(&app, &stored)
    }

    #[tauri::command]
    pub fn evening_wrap(_app: tauri::AppHandle) -> Result<WrapView, String> {
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
