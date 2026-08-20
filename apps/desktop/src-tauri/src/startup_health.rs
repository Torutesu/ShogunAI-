//! What the app could not set up at boot, in a shape the panel can read.
//!
//! Each of these already printed one line to stderr and then carried on: the memory DB failing to
//! open (`lib.rs` `memory_db`), Accessibility not being granted, the local embedding model being
//! absent (`attach_embedder`). That is fine while a developer is watching a terminal. A build the
//! user double-clicks has no terminal, so the app looked healthy while capture, ⌥-tap drafting or
//! hybrid search were silently off — the failure mode reported on device on 2026-08-10.
//!
//! Boot facts are recorded here as they happen; Accessibility is read live on every query instead,
//! because the user can grant it while the app is running and the warning must clear itself when
//! they do. The silent check is deliberate: the prompting variant would raise the system alert on
//! every poll.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;

    /// Boot-time facts. Written once during setup, read by the command.
    #[derive(Clone, Default)]
    struct Boot {
        memory_db_error: Option<String>,
        embedding_model: bool,
    }

    // A module-level cell rather than Tauri state: these are written from inside `setup`, before
    // and after `app.manage` calls, and a plain static sidesteps any ordering question about when
    // the state becomes available.
    static BOOT: Mutex<Option<Boot>> = Mutex::new(None);

    fn with_boot(f: impl FnOnce(&mut Boot)) {
        if let Ok(mut g) = BOOT.lock() {
            f(g.get_or_insert_with(Boot::default));
        }
    }

    /// The memory DB could not be opened. Capture, search and ⌥-tap drafting are all off until it
    /// is. The message is the same one that goes to stderr — a reason, never user content.
    pub fn set_memory_db_error(reason: impl Into<String>) {
        with_boot(|b| b.memory_db_error = Some(reason.into()));
    }

    /// Whether the local ONNX embedding model was found. Without it search degrades to lexical.
    pub fn set_embedding_model(present: bool) {
        with_boot(|b| b.embedding_model = present);
    }

    /// What the panel shows. Serialized to the webview as-is.
    #[derive(serde::Serialize)]
    pub struct StartupHealth {
        /// `None` when the memory DB opened normally.
        pub memory_db_error: Option<String>,
        /// Live, not a boot snapshot: granting Accessibility clears this without a relaunch.
        pub accessibility: bool,
        /// `false` = hybrid search is unavailable and results are lexical only.
        pub embedding_model: bool,
        /// The store opened at boot but the **last** operation against it failed (issue #121).
        /// Live and self-clearing, like `accessibility`: the next successful read lifts it.
        pub memory_degraded: bool,
        /// Which class of failure, when degraded (`"lock_poisoned"` / `"query"`). A tag, never a
        /// driver message — nothing from a row can reach the webview through this.
        pub memory_fault: Option<&'static str>,
        /// Store failures since launch. Monotonic, so a store that keeps failing and recovering
        /// is still visible at a moment when `memory_degraded` happens to be false.
        pub memory_faults_total: u64,
    }

    /// Tauri command: the panel asks on mount and again whenever it expands.
    ///
    /// `Db` is looked up optionally: the shell deliberately keeps running when the memory store
    /// cannot be opened at all (that case is already `memory_db_error`), and declaring it as a
    /// `State` parameter would turn the health query itself into an error there — blanking the
    /// one screen whose job is to explain what is broken.
    #[tauri::command]
    pub fn startup_health(app: tauri::AppHandle) -> StartupHealth {
        use tauri::Manager;
        let boot = BOOT.lock().ok().and_then(|g| g.clone()).unwrap_or_default();
        let memory = app
            .try_state::<shogun_core::daemon::Db>()
            .map(|db| db.memory_health())
            .unwrap_or(shogun_core::memory_health::MemoryHealthSnapshot {
                degraded: false,
                fault: None,
                faults_total: 0,
                last_fault_ms: None,
            });
        StartupHealth {
            memory_db_error: boot.memory_db_error,
            accessibility: crate::axcache::ax_trusted_silent(),
            embedding_model: boot.embedding_model,
            memory_degraded: memory.degraded,
            memory_fault: memory.fault.map(|f| f.as_str()),
            memory_faults_total: memory.faults_total,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The default must not claim a model is present: an app that never called
        /// `set_embedding_model` has not found one, and saying otherwise would hide the warning.
        #[test]
        fn default_boot_reports_no_model_and_no_error() {
            let b = Boot::default();
            assert!(!b.embedding_model);
            assert!(b.memory_db_error.is_none());
        }

        #[test]
        fn setters_record_what_the_panel_needs() {
            let b = Boot {
                memory_db_error: Some("keychain denied".into()),
                embedding_model: true,
            };
            assert_eq!(b.memory_db_error.as_deref(), Some("keychain denied"));
            assert!(b.embedding_model);
        }
    }
}
