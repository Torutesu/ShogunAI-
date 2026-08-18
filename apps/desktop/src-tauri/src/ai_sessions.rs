//! Importing local AI coding-tool session logs into memory (Phase R4, macOS side).
//!
//! The parsing is pure and lives in `shogun_memory::ai_session`; this file is the effectful half:
//! find the logs, read them, and hand the turns to the daemon. Kept small and defensive — an
//! unreadable or half-written file must never take the app down or stop the other files importing.
//!
//! Opt-in by design: nothing is imported unless the user turns it on, because a session log is a
//! transcript of their work and some of it is not theirs to hand over.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use shogun_core::daemon::Db;

    /// How often to re-scan for new turns. Session logs are appended to as the user works, and
    /// re-importing is idempotent (the daemon touches already-seen turns), so a steady poll costs
    /// little and keeps memory current without a file watcher.
    pub const DEFAULT_SCAN_INTERVAL_MS: u64 = 5 * 60 * 1000;

    /// Cap on how much of one log is read per pass. A long-running session can reach tens of MB;
    /// this keeps a scan bounded, and the next pass picks up whatever is new.
    const MAX_BYTES_PER_FILE: u64 = 32 * 1024 * 1024;

    /// The setting that gates importing (default off — see the module note).
    pub fn is_enabled(app: &tauri::AppHandle) -> bool {
        settings_path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| s.trim() == "on")
            .unwrap_or(false)
    }

    /// Turn importing on or off. Returns the new state.
    #[tauri::command]
    pub fn set_ai_session_import(enabled: bool, app: tauri::AppHandle) -> Result<bool, String> {
        let path = settings_path(&app).ok_or("no app data dir")?;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&path, if enabled { "on" } else { "off" }).map_err(|e| e.to_string())?;
        eprintln!(
            "[ai-sessions] import {}",
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(enabled)
    }

    /// Whether importing is currently on, for the settings UI.
    #[tauri::command]
    pub fn get_ai_session_import(app: tauri::AppHandle) -> bool {
        is_enabled(&app)
    }

    fn settings_path(app: &tauri::AppHandle) -> Option<PathBuf> {
        use tauri::Manager;
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("ai-session-import"))
    }

    /// The directories session logs live in. Only Claude Code's layout is known today; the others
    /// are added here as their formats are confirmed rather than guessed at.
    fn log_roots() -> Vec<PathBuf> {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return Vec::new();
        };
        vec![home.join(".claude/projects")]
    }

    /// Every `.jsonl` under `root`, one level of project directories deep.
    fn logs_under(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(projects) = std::fs::read_dir(root) else {
            return out;
        };
        for project in projects.flatten() {
            let Ok(files) = std::fs::read_dir(project.path()) else {
                continue;
            };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().is_some_and(|e| e == "jsonl") {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Import one log file. Returns how many turns were newly stored.
    fn import_file(db: &Db, path: &Path) -> usize {
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_BYTES_PER_FILE {
            eprintln!("[ai-sessions] skipping oversized log: {}", path.display());
            return 0;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return 0;
        };
        let turns: Vec<_> = text
            .lines()
            .filter_map(shogun_memory::ai_session::parse_claude_code_line)
            .collect();
        if turns.is_empty() {
            return 0;
        }
        db.ingest_ai_session(&turns).newly_inserted
    }

    /// Scan every known log once. Returns the total newly stored.
    pub fn scan_once(db: &Db) -> usize {
        let mut total = 0;
        for root in log_roots() {
            for log in logs_under(&root) {
                total += import_file(db, &log);
            }
        }
        total
    }

    /// Start the periodic import. The thread checks the setting on every pass, so toggling it in
    /// Settings takes effect without a restart.
    pub fn spawn_importer(app: tauri::AppHandle, db: Db) {
        std::thread::spawn(move || loop {
            if is_enabled(&app) {
                let n = scan_once(&db);
                if n > 0 {
                    eprintln!("[ai-sessions] imported {n} new turn(s)");
                }
            }
            std::thread::sleep(Duration::from_millis(DEFAULT_SCAN_INTERVAL_MS));
        });
    }
}
