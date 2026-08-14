//! Desktop liveness heartbeat consumed by standalone MCP tool calls.

#[cfg(target_os = "macos")]
pub(crate) fn start(app: &tauri::App) {
    use tauri::Manager;

    let Ok(base) = app.path().app_data_dir() else {
        eprintln!("[heartbeat] app data directory unavailable");
        return;
    };
    let path = crate::memory_data_dir(base).join(shogun_mcp::desktop_heartbeat::FILE_NAME);
    std::thread::spawn(move || loop {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        if let Err(error) = shogun_mcp::desktop_heartbeat::write(&path, now_ms) {
            eprintln!("[heartbeat] write failed: {error}");
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    });
}
