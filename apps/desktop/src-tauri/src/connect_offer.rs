//! "Connect this app" offer (issue #86): when the user brings an app to the front whose service
//! SHOGUN can integrate with but hasn't been connected yet, the notch offers a one-click connect
//! (No / Not now / Yes).
//!
//! Detection rides the meeting driver's existing 1s frontmost tick (`spawn_meeting_driver` calls
//! [`mac::on_front`]) — one poll, two features, no thread of its own. An offer stands only while
//! the service is Disconnected on a transport that can actually serve it, so on a default build
//! this fires for nothing until a wave ships its transport (today: Google Calendar behind
//! `SHOGUN_ENABLE_WAVE1_READ`); each mapped service starts offering automatically the moment its
//! transport goes live.
//!
//! Answer semantics:
//! - **Not now** — a real ten-minute snooze per app. Deliberately NOT the meeting lane's
//!   `OfferGate`: its away-clear rule ("60s out of front = the meeting ended, ask again") encodes
//!   a meeting fact with no analogue here — leaving Slack for a minute is normal multitasking,
//!   not a change of mind about connecting Slack.
//! - **No** — persisted per *service* in `connect_offer.json`, never offered again (the
//!   connections screen in Settings remains the way to connect later).
//! - **Yes** — the webview runs the existing `connect_service` flow; this module never connects
//!   anything itself.
//! - **Silence** — an offer ignored for [`mac::OFFER_TTL_MS`] is an implicit "Not now": the chin
//!   has other jobs (health warnings, the greeting) and must not be occupied indefinitely.
//!
//! Rust owns the lifecycle and pushes the pill via the `connect_offer` event (`None` clears);
//! the webview never decides that an offer exists — same push-driven contract as the meeting
//! pill (FR-MT-07's shape).

#[cfg(target_os = "macos")]
pub mod mac {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use serde::{Deserialize, Serialize};
    use shogun_mcp::scope::{from_source, Service};
    use tauri::{Emitter, Manager};

    /// How long "Not now" (and an ignored offer) silences the same app.
    pub const COOLDOWN_MS: i64 = 10 * 60 * 1_000;
    /// How long an unanswered offer may occupy the chin before it stands down as an implicit
    /// "Not now".
    pub const OFFER_TTL_MS: i64 = 30 * 1_000;

    /// What the webview renders: which service to offer, triggered by which app. The webview
    /// keys labels off `service` the same way the connections screen does.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    pub struct Offer {
        /// Stable service id (`gcal` / `slack` / `notion` / …).
        pub service: String,
        /// The app that triggered the offer — what "Not now" declines.
        pub bundle_id: String,
    }

    /// Native app bundle → the service its data lives in. Web-app services (Gmail, Drive) have
    /// no bundle id to key off — offering them needs browser-tab awareness, out of scope for the
    /// native-app trigger (#86).
    pub fn service_for_bundle(bundle_id: &str) -> Option<Service> {
        match bundle_id {
            // Apple Calendar is where a Google calendar is usually *viewed* — the connector the
            // offer points at is Google Calendar (same mapping the timeline uses in appIcons.ts).
            "com.apple.iCal" => Some(Service::GoogleCalendar),
            "com.tinyspeck.slackmacgap" => Some(Service::Slack),
            "notion.id" => Some(Service::Notion),
            "com.linear" => Some(Service::Linear),
            "com.github.GitHubClient" => Some(Service::GitHub),
            _ => None,
        }
    }

    struct Inner {
        /// bundle id → the moment its "Not now" lapses. No away-clear (see module doc).
        declined_until: HashMap<String, i64>,
        /// Service sources the user said "No" to — persisted, never offered again.
        dismissed: HashSet<String>,
        /// What the webview currently shows (`None` = nothing).
        current: Option<Offer>,
        /// When `current` last became `Some` — drives the ignore-TTL.
        shown_since_ms: i64,
    }

    pub struct ConnectOfferState(Mutex<Inner>);

    /// The offer an app would get right now, before connectedness is considered: mapped, not
    /// permanently dismissed, not snoozed.
    fn desired_offer(
        bundle_id: &str,
        dismissed: &HashSet<String>,
        declined_until: &HashMap<String, i64>,
        now: i64,
    ) -> Option<Offer> {
        let svc = service_for_bundle(bundle_id)?;
        if dismissed.contains(svc.source_str()) {
            return None;
        }
        if declined_until.get(bundle_id).is_some_and(|until| now < *until) {
            return None;
        }
        Some(Offer {
            service: svc.source_str().to_string(),
            bundle_id: bundle_id.to_string(),
        })
    }

    /// One evaluation of the nag rules, pure so they are testable: given the frontmost bundle
    /// and whether its service is offerable-disconnected, mutate the state and return
    /// `Some(payload)` when the webview must be told (`Some(None)` clears the pill), `None` when
    /// nothing changed.
    fn step(inner: &mut Inner, bundle_id: &str, offerable: bool, now: i64) -> Option<Option<Offer>> {
        let desired = if offerable {
            desired_offer(bundle_id, &inner.dismissed, &inner.declined_until, now)
        } else {
            None
        };
        if desired == inner.current {
            // A standing, unanswered offer eventually stands down as an implicit "Not now" —
            // the chin has other jobs (health warnings, the greeting).
            if inner.current.is_some() && now - inner.shown_since_ms >= OFFER_TTL_MS {
                inner.declined_until.insert(bundle_id.to_string(), now + COOLDOWN_MS);
                inner.current = None;
                return Some(None);
            }
            return None;
        }
        if desired.is_some() {
            inner.shown_since_ms = now;
        }
        inner.current = desired.clone();
        Some(desired)
    }

    // ------------------------------------------------------- persistence ("No" only)

    #[derive(Default, Serialize, Deserialize)]
    struct Stored {
        #[serde(default)]
        dismissed: HashSet<String>,
    }

    fn store_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("connect_offer.json"))
    }

    fn load_dismissed(app: &tauri::AppHandle) -> HashSet<String> {
        let Some(path) = store_path(app) else {
            return HashSet::new();
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return HashSet::new();
        };
        serde_json::from_str::<Stored>(&raw)
            .map(|s| s.dismissed)
            .unwrap_or_default()
    }

    fn save_dismissed(app: &tauri::AppHandle, dismissed: &HashSet<String>) -> Result<(), String> {
        let path = store_path(app).ok_or("app data dir unavailable")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create data dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(&Stored {
            dismissed: dismissed.clone(),
        })
        .map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("write connect_offer: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("commit connect_offer: {e}"))?;
        Ok(())
    }

    // ------------------------------------------------------------------ driver

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Register state. Ticks arrive from the meeting driver's 1s frontmost poll — see module doc.
    pub fn install(app: &tauri::AppHandle) {
        app.manage(ConnectOfferState(Mutex::new(Inner {
            declined_until: HashMap::new(),
            dismissed: load_dismissed(app),
            current: None,
            shown_since_ms: 0,
        })));
    }

    /// One tick with the frontmost bundle id. Called every second by the meeting driver.
    pub fn on_front(app: &tauri::AppHandle, bundle_id: &str) {
        let Some(state) = app.try_state::<ConnectOfferState>() else {
            return;
        };
        let now = now_ms();
        // Connectedness is read BEFORE taking the offer lock: the connector mutex is held across
        // network I/O by the 15-minute sync poller, and the offer lock is what the webview's
        // commands wait on — a "Not now" click must not beachball behind a slow sync.
        let offerable = service_for_bundle(bundle_id)
            .is_some_and(|svc| crate::connectors::mac::offerable_disconnected(app, svc, now));
        let Ok(mut inner) = state.0.lock() else {
            return;
        };
        let emit = step(&mut inner, bundle_id, offerable, now);
        drop(inner);
        if let Some(payload) = emit {
            let _ = app.emit("connect_offer", &payload);
        }
    }

    // ---------------------------------------------------------------- commands

    /// What the pill should show right now — the webview's recovery read after a reload,
    /// mirroring `meeting_status`.
    #[tauri::command]
    pub fn connect_offer_status(state: tauri::State<'_, ConnectOfferState>) -> Option<Offer> {
        state.0.lock().ok().and_then(|inner| inner.current.clone())
    }

    /// "Not now": a ten-minute snooze for this app. The webview clears its pill locally; the
    /// next tick re-derives `None` from the cooldown, so this only has to record the answer.
    #[tauri::command]
    pub fn connect_offer_not_now(bundle_id: String, state: tauri::State<'_, ConnectOfferState>) {
        if let Ok(mut inner) = state.0.lock() {
            inner.declined_until.insert(bundle_id, now_ms() + COOLDOWN_MS);
            inner.current = None;
        }
    }

    /// "No": never offer this service again (persisted per service, not per app — saying no to
    /// Slack in the Slack app is saying no to Slack).
    #[tauri::command]
    pub fn connect_offer_never(
        service: String,
        app: tauri::AppHandle,
        state: tauri::State<'_, ConnectOfferState>,
    ) -> Result<(), String> {
        let svc = from_source(&service).ok_or_else(|| format!("unknown service: {service}"))?;
        let dismissed = {
            let mut inner = state
                .0
                .lock()
                .map_err(|_| "offer state lock poisoned".to_string())?;
            inner.dismissed.insert(svc.source_str().to_string());
            inner.current = None;
            inner.dismissed.clone()
        };
        save_dismissed(&app, &dismissed).map_err(|e| {
            // The in-memory "No" still holds for this session; say out loud why it will not
            // survive a relaunch instead of letting the pill quietly come back one day.
            eprintln!("[connect-offer] persisting dismissal failed: {e}");
            e
        })
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use super::*;

        const SLACK: &str = "com.tinyspeck.slackmacgap";
        const NOTION: &str = "notion.id";

        fn fresh() -> Inner {
            Inner {
                declined_until: HashMap::new(),
                dismissed: HashSet::new(),
                current: None,
                shown_since_ms: 0,
            }
        }

        fn slack_offer() -> Offer {
            Offer {
                service: "slack".into(),
                bundle_id: SLACK.into(),
            }
        }

        #[test]
        fn maps_known_bundles_and_ignores_the_rest() {
            assert_eq!(service_for_bundle(SLACK), Some(Service::Slack));
            assert_eq!(service_for_bundle(NOTION), Some(Service::Notion));
            assert_eq!(service_for_bundle("com.apple.finder"), None);
        }

        #[test]
        fn offers_a_disconnected_mapped_app_once() {
            let mut inner = fresh();
            assert_eq!(step(&mut inner, SLACK, true, 1_000), Some(Some(slack_offer())));
            // Steady state: same app, same answer — no re-emit spam.
            assert_eq!(step(&mut inner, SLACK, true, 2_000), None);
        }

        #[test]
        fn no_offer_when_already_connected_or_unmapped() {
            let mut inner = fresh();
            assert_eq!(step(&mut inner, SLACK, false, 1_000), None);
            assert_eq!(step(&mut inner, "com.apple.finder", true, 1_000), None);
        }

        #[test]
        fn not_now_holds_the_full_cooldown_even_across_app_switches() {
            // The reason this module does NOT reuse the meeting OfferGate: its away-clear rule
            // would void the snooze after 60s in another app. Here the decline must survive
            // ordinary multitasking.
            let mut inner = fresh();
            inner.declined_until.insert(SLACK.to_string(), 1_000 + COOLDOWN_MS);
            assert_eq!(step(&mut inner, SLACK, true, 1_000), None);
            // Two minutes in the editor, then back — still declined.
            assert_eq!(step(&mut inner, "com.apple.dt.Xcode", false, 60_000), None);
            assert_eq!(step(&mut inner, SLACK, true, 121_000), None);
            // The cooldown lapsing is what re-offers.
            assert_eq!(
                step(&mut inner, SLACK, true, 1_000 + COOLDOWN_MS),
                Some(Some(slack_offer()))
            );
        }

        #[test]
        fn no_suppresses_forever() {
            let mut inner = fresh();
            inner.dismissed.insert("slack".to_string());
            assert_eq!(step(&mut inner, SLACK, true, i64::MAX - COOLDOWN_MS), None);
        }

        #[test]
        fn an_ignored_offer_stands_down_and_snoozes() {
            let mut inner = fresh();
            assert_eq!(step(&mut inner, SLACK, true, 1_000), Some(Some(slack_offer())));
            assert_eq!(step(&mut inner, SLACK, true, 1_000 + OFFER_TTL_MS - 1), None);
            // TTL reached: the pill clears and the app is snoozed as an implicit "Not now".
            assert_eq!(step(&mut inner, SLACK, true, 1_000 + OFFER_TTL_MS), Some(None));
            assert_eq!(step(&mut inner, SLACK, true, 2_000 + OFFER_TTL_MS), None);
        }

        #[test]
        fn switching_to_another_mapped_app_swaps_the_offer_and_resets_the_ttl() {
            let mut inner = fresh();
            assert_eq!(step(&mut inner, SLACK, true, 1_000), Some(Some(slack_offer())));
            let swapped = step(&mut inner, NOTION, true, 1_000 + OFFER_TTL_MS - 1);
            assert_eq!(
                swapped,
                Some(Some(Offer {
                    service: "notion".into(),
                    bundle_id: NOTION.into()
                }))
            );
            // The TTL clock restarted with the swap.
            assert_eq!(step(&mut inner, NOTION, true, 1_500 + OFFER_TTL_MS), None);
        }

        #[test]
        fn leaving_the_mapped_app_clears_the_pill() {
            let mut inner = fresh();
            assert_eq!(step(&mut inner, SLACK, true, 1_000), Some(Some(slack_offer())));
            assert_eq!(step(&mut inner, "com.apple.finder", false, 2_000), Some(None));
        }

        #[test]
        fn stored_missing_field_defaults_to_empty() {
            let back: Stored = serde_json::from_str("{}").unwrap();
            assert!(back.dismissed.is_empty());
        }
    }
}
