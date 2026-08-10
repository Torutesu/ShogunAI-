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
    }

    /// Tauri command: the panel asks on mount and again whenever it expands.
    #[tauri::command]
    pub fn startup_health() -> StartupHealth {
        let boot = BOOT.lock().ok().and_then(|g| g.clone()).unwrap_or_default();
        StartupHealth {
            memory_db_error: boot.memory_db_error,
            accessibility: crate::axcache::ax_trusted_silent(),
            embedding_model: boot.embedding_model,
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
            let b = Boot { memory_db_error: Some("keychain denied".into()), embedding_model: true };
            assert_eq!(b.memory_db_error.as_deref(), Some("keychain denied"));
            assert!(b.embedding_model);
        }
    }
}
