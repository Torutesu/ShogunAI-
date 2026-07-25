//! Capture exclusions the user controls (FR-CAP-06, macOS side).
//!
//! SHOGUN reads whatever is on screen, so "don't look at this one" has to be something the user
//! can actually say. The policy had non-removable defaults (password managers, private browsing)
//! and mutators, but no persistence and no way to reach them — this is that half.
//!
//! The policy is shared with the capture poller, so a change takes effect on the next tick rather
//! than at the next launch. Somebody excluding an app is usually reacting to what is on their
//! screen *right now*, and "it will stop reading it tomorrow" is not an answer.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::{Arc, Mutex};

    use shogun_core::capture::exclusion::{is_default_excluded, ExclusionPolicy};
    use shogun_core::daemon::Db;

    /// The policy shared between the settings commands and the capture poller.
    pub type SharedPolicy = Arc<Mutex<ExclusionPolicy>>;

    /// The one policy for the process, reachable from code that cannot be handed Tauri state.
    ///
    /// The AX cache warmer runs on its own thread, started before any command exists, and it reads
    /// the focused window's text just like the capture poller does — so it has to consult the same
    /// exclusions. Threading the handle through would mean reordering startup around it; a single
    /// process-wide policy is what this actually is.
    static POLICY: std::sync::OnceLock<SharedPolicy> = std::sync::OnceLock::new();

    /// Publish the policy. Called once during setup, before the watchers start.
    pub fn install(policy: SharedPolicy) {
        let _ = POLICY.set(policy);
    }

    /// The installed policy, or `None` before setup has published it.
    ///
    /// Callers that are about to *read* a window must treat `None` as "do not read". Startup is
    /// the only window where it is unset, and being briefly blind is the right failure: the
    /// alternative is reading a password manager because a thread started a few milliseconds early.
    pub fn shared() -> Option<&'static SharedPolicy> {
        POLICY.get()
    }

    /// Whether this focus must not be read at all. Fails closed on a missing or poisoned policy.
    pub fn is_excluded(bundle_id: &str, window_title: Option<&str>) -> bool {
        match shared().map(|p| p.lock()) {
            Some(Ok(policy)) => policy.is_excluded(bundle_id, window_title).is_some(),
            _ => true,
        }
    }

    fn store_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        app.path().app_data_dir().ok().map(|d| d.join("exclusions.json"))
    }

    /// Load the user's exclusions, layered onto the non-removable defaults.
    pub fn load(app: &tauri::AppHandle) -> ExclusionPolicy {
        let mut policy = ExclusionPolicy::new();
        let Some(path) = store_path(app) else { return policy };
        let Ok(text) = std::fs::read_to_string(path) else { return policy };
        if let Ok(apps) = serde_json::from_str::<Vec<String>>(&text) {
            for a in apps {
                policy.add_app(a);
            }
        }
        policy
    }

    fn save(app: &tauri::AppHandle, policy: &ExclusionPolicy) -> Result<(), String> {
        let path = store_path(app).ok_or("no app data dir")?;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let apps: Vec<&str> = policy.user_apps();
        let json = serde_json::to_string_pretty(&apps).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// One row in the exclusions UI.
    #[derive(serde::Serialize)]
    pub struct ExclusionRow {
        pub bundle_id: String,
        /// Currently excluded from capture.
        pub excluded: bool,
        /// A built-in exclusion: always on, and the UI must not offer to turn it off.
        pub locked: bool,
        /// How many events SHOGUN has captured from it (0 for a default never seen).
        pub events: i64,
    }

    /// The apps the user can decide about: everything captured so far, plus the built-in
    /// exclusions and anything they have already excluded (which by definition stopped appearing
    /// in the capture log, so it would otherwise vanish from its own settings row).
    #[tauri::command]
    pub fn list_exclusions(
        db: tauri::State<'_, Db>,
        policy: tauri::State<'_, SharedPolicy>,
    ) -> Vec<ExclusionRow> {
        let seen = db.captured_apps(40);
        let guard = policy.lock().ok();
        let user: Vec<String> = guard
            .as_ref()
            .map(|p| p.user_apps().into_iter().map(str::to_string).collect())
            .unwrap_or_default();

        let mut rows: Vec<ExclusionRow> = Vec::new();
        let mut push = |bundle_id: String, events: i64| {
            if rows.iter().any(|r| r.bundle_id == bundle_id) {
                return;
            }
            let locked = is_default_excluded(&bundle_id);
            let excluded = locked || user.iter().any(|u| *u == bundle_id);
            rows.push(ExclusionRow { bundle_id, excluded, locked, events });
        };
        for (bundle, count) in seen {
            push(bundle, count);
        }
        for u in &user {
            push(u.clone(), 0);
        }
        rows
    }

    /// Exclude or re-include an app. A built-in exclusion cannot be turned off (FR-CAP-06) and the
    /// attempt is refused rather than silently ignored, so the UI can say why.
    #[tauri::command]
    pub fn set_app_excluded(
        bundle_id: String,
        excluded: bool,
        app: tauri::AppHandle,
        policy: tauri::State<'_, SharedPolicy>,
    ) -> Result<(), String> {
        let bundle_id = bundle_id.trim().to_string();
        if bundle_id.is_empty() {
            return Err("no app given".into());
        }
        if is_default_excluded(&bundle_id) && !excluded {
            return Err("this app is always excluded and can't be turned back on".into());
        }
        {
            let mut guard = policy.lock().map_err(|_| "exclusion policy lock poisoned")?;
            if excluded {
                guard.add_app(bundle_id.clone());
            } else {
                guard.remove_app(&bundle_id);
            }
            save(&app, &guard)?;
        }
        eprintln!(
            "[exclusions] {bundle_id} {}",
            if excluded { "excluded from capture" } else { "re-included" }
        );
        Ok(())
    }
}
