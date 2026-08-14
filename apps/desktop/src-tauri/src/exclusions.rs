//! Capture exclusions, macOS side (FR-CAP-05/06).
//!
//! SHOGUN reads whatever is on screen, so what it must *not* read has to be decided somewhere —
//! and decided in one place, because more than one thread reads the focused window. This publishes
//! the single policy those readers consult.
//!
//! **No settings UI, deliberately.** Per-app on/off switches asked the user to curate a list of
//! bundle identifiers to answer a question the product should answer itself. The categories that
//! matter — password managers, the authentication agent, terminals, private browsing — are
//! non-removable defaults that apply without being configured. A `exclusions.json` in the app data
//! directory still layers extra apps on top for anyone who wants them; nothing writes it today.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::{Arc, Mutex};

    use shogun_core::capture::exclusion::ExclusionPolicy;

    /// The policy shared between every reader of the focused window.
    pub type SharedPolicy = Arc<Mutex<ExclusionPolicy>>;

    /// The one policy for the process, reachable from code that cannot be handed Tauri state.
    ///
    /// The AX cache warmer runs on its own thread, started before any Tauri state exists, and it
    /// reads the focused window's text just like the capture poller does — so it has to consult the
    /// same exclusions. Threading the handle through would mean reordering startup around it; a
    /// single process-wide policy is what this actually is.
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
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("exclusions.json"))
    }

    /// Load the user's exclusions, layered onto the non-removable defaults.
    pub fn load(app: &tauri::AppHandle) -> ExclusionPolicy {
        let mut policy = ExclusionPolicy::new();
        let Some(path) = store_path(app) else {
            return policy;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return policy;
        };
        if let Ok(apps) = serde_json::from_str::<Vec<String>>(&text) {
            for a in apps {
                policy.add_app(a);
            }
        }
        policy
    }
}
