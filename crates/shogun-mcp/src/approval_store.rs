//! Cross-process L3 approval store (§6.6 / FR-API-04).
//!
//! Standalone `shogun-mcp` and the desktop app are separate processes. Each used to keep an
//! in-memory [`ApprovalQueue`], so an MCP `actions.execute` send never appeared in Settings →
//! Approvals. This module persists the queue to `l3_approvals.json` next to `memory.db` so both
//! faces share one confirm path: MCP enqueues → desktop lists/confirm/reject → send executes.
//!
//! Atomic replace via temp file (same pattern as `memory_api.json`). Best-effort under concurrent
//! writers; last writer wins. Content is send previews awaiting human confirm — not secrets.

use std::path::{Path, PathBuf};
use std::time::Duration;

use shogun_agents::approval::{
    ApprovalId, ApprovalQueue, ApprovalStatus, KeyKind, Origin, PendingRecord, Preview, Route,
    TerminalRecord,
};
use shogun_agents::permission::SendAction;

/// Filename next to `memory.db` / app-data.
pub const STORE_FILE: &str = "l3_approvals.json";

/// Env override for the store path (standalone `shogun-mcp`).
pub const STORE_ENV: &str = "SHOGUN_L3_APPROVALS";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WireStore {
    #[serde(default = "default_next_id")]
    next_id: u64,
    #[serde(default)]
    pending: Vec<WirePending>,
    #[serde(default)]
    terminal: Vec<WireTerminal>,
    #[serde(default)]
    in_flight: Vec<u64>,
}

fn default_next_id() -> u64 {
    1
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WirePending {
    id: u64,
    kind: String,
    destination: String,
    full_body: String,
    route: String,
    origin: String,
    created_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WireTerminal {
    id: u64,
    status: String,
    resolved_ms: u64,
}

impl Default for WireStore {
    fn default() -> Self {
        Self { next_id: 1, pending: Vec::new(), terminal: Vec::new(), in_flight: Vec::new() }
    }
}

/// Resolve `l3_approvals.json`: `SHOGUN_L3_APPROVALS`, else next to the DB path's parent.
pub fn resolve_store_path(db_path: &str) -> PathBuf {
    if let Ok(p) = std::env::var(STORE_ENV) {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    Path::new(db_path)
        .parent()
        .map(|p| p.join(STORE_FILE))
        .unwrap_or_else(|| PathBuf::from(STORE_FILE))
}

fn route_wire(route: Route) -> &'static str {
    match route {
        Route::DirectMcp => "direct",
        Route::ViaComposio => "composio",
    }
}

fn route_parse(s: &str) -> Result<Route, String> {
    match s {
        "direct" => Ok(Route::DirectMcp),
        "composio" => Ok(Route::ViaComposio),
        _ => Err(format!("invalid approval route: {s}")),
    }
}

fn origin_wire(origin: Origin) -> &'static str {
    match origin {
        Origin::Human => "human",
        Origin::AiApi => "ai_api",
    }
}

fn origin_parse(s: &str) -> Result<Origin, String> {
    match s {
        "human" => Ok(Origin::Human),
        "ai_api" => Ok(Origin::AiApi),
        _ => Err(format!("invalid approval origin: {s}")),
    }
}

fn status_wire(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Rejected => "rejected",
        ApprovalStatus::TimedOut => "timed_out",
        ApprovalStatus::Sent => "sent",
        ApprovalStatus::SendFailed => "send_failed",
        ApprovalStatus::DraftSaved => "draft_saved",
    }
}

fn status_parse(s: &str) -> Result<ApprovalStatus, String> {
    match s {
        "pending" => Ok(ApprovalStatus::Pending),
        "rejected" => Ok(ApprovalStatus::Rejected),
        "timed_out" => Ok(ApprovalStatus::TimedOut),
        "sent" => Ok(ApprovalStatus::Sent),
        "send_failed" => Ok(ApprovalStatus::SendFailed),
        "draft_saved" => Ok(ApprovalStatus::DraftSaved),
        _ => Err(format!("invalid approval status: {s}")),
    }
}

fn kind_wire(action: &SendAction) -> &'static str {
    match action {
        SendAction::SendEmail { .. } => "send_email",
        SendAction::PostMessage { .. } => "post_message",
        SendAction::CreateCalendarEvent { .. } => "create_calendar_event",
        SendAction::PostComment { .. } => "post_comment",
    }
}

fn action_from_wire(kind: &str, destination: String) -> Result<SendAction, String> {
    if destination.trim().is_empty() { return Err("approval destination is empty".into()); }
    Ok(match kind {
        "send_email" => SendAction::SendEmail { to: destination },
        "post_message" => SendAction::PostMessage { channel: destination },
        "create_calendar_event" => SendAction::CreateCalendarEvent { title: destination },
        "post_comment" => SendAction::PostComment { target: destination },
        _ => return Err(format!("invalid approval action kind: {kind}")),
    })
}

fn to_wire(record: &PendingRecord) -> WirePending {
    WirePending {
        id: record.id.0,
        kind: kind_wire(&record.action).to_string(),
        destination: record.preview.destination.clone(),
        full_body: record.preview.full_body.clone(),
        route: route_wire(record.preview.route).to_string(),
        origin: origin_wire(record.origin).to_string(),
        created_ms: record.created_ms,
    }
}

fn from_wire(w: WirePending) -> Result<PendingRecord, String> {
    if w.id == 0 { return Err("approval id must be positive".into()); }
    let action = action_from_wire(&w.kind, w.destination.clone())?;
    let route = route_parse(&w.route)?;
    let origin = origin_parse(&w.origin)?;
    let expected_route = match &action { SendAction::SendEmail { .. } => Route::ViaComposio, _ => Route::DirectMcp };
    if route != expected_route { return Err("approval route does not match action".into()); }
    let preview = Preview {
        op_type: match &action {
            SendAction::SendEmail { .. } => "Send email",
            SendAction::PostMessage { .. } => "Post message",
            SendAction::CreateCalendarEvent { .. } => "Create calendar event",
            SendAction::PostComment { .. } => "Post comment",
        },
        destination: w.destination,
        full_body: w.full_body,
        route,
        key_kind: KeyKind::Byok,
    };
    Ok(PendingRecord {
        id: ApprovalId(w.id),
        action,
        preview,
        origin,
        created_ms: w.created_ms,
    })
}

fn queue_to_wire(q: &ApprovalQueue) -> WireStore {
    let (next_id, records) = q.export();
    WireStore {
        next_id,
        pending: records.iter().map(to_wire).collect(),
        terminal: q.terminal_records().iter().map(|r| WireTerminal {
            id: r.id.0,
            status: status_wire(r.status).to_string(),
            resolved_ms: r.resolved_ms,
        }).collect(),
        in_flight: q.in_flight_ids().iter().map(|id| id.0).collect(),
    }
}

fn queue_from_wire(store: WireStore) -> Result<ApprovalQueue, String> {
    let mut ids = std::collections::HashSet::new();
    let mut records = Vec::with_capacity(store.pending.len());
    for row in store.pending { if !ids.insert(row.id) { return Err("duplicate approval id".into()); } records.push(from_wire(row)?); }
    let mut terminal = Vec::with_capacity(store.terminal.len());
    for r in store.terminal {
        if r.id == 0 || !ids.insert(r.id) { return Err("duplicate or invalid terminal approval id".into()); }
        let status = status_parse(&r.status)?;
        if status == ApprovalStatus::Pending { return Err("terminal approval cannot be pending".into()); }
        terminal.push(TerminalRecord {
        id: ApprovalId(r.id),
        status,
        resolved_ms: r.resolved_ms,
        });
    }
    let mut in_flight = Vec::with_capacity(store.in_flight.len());
    for id in store.in_flight {
        if id == 0 || !ids.insert(id) { return Err("duplicate or invalid in-flight approval id".into()); }
        in_flight.push(ApprovalId(id));
    }
    Ok(ApprovalQueue::import_with_terminal(store.next_id, records, terminal, in_flight))
}

/// Load shared queue. Missing file means empty; corrupt/unreadable file is an error.
pub fn load_queue(path: &Path) -> Result<ApprovalQueue, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<WireStore>(&text)
            .map_err(|e| format!("load l3 approvals: {e}"))
            .and_then(|store| queue_from_wire(store))
            .map_err(|e| format!("load l3 approvals: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ApprovalQueue::new()),
        Err(e) => Err(format!("read l3 approvals: {e}")),
    }
}

struct StoreLock { file: std::fs::File }

impl Drop for StoreLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.file), libc::LOCK_UN); }
    }
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.display()))
}

fn acquire_lock(path: &Path) -> Result<StoreLock, String> {
    let lock = lock_path(path);
    for _ in 0..500 {
        match std::fs::OpenOptions::new().read(true).write(true).create(true).open(&lock) {
            Ok(file) => {
                #[cfg(unix)]
                {
                    let result = unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX | libc::LOCK_NB) };
                    if result == 0 { return Ok(StoreLock { file }); }
                    if std::io::Error::last_os_error().kind() != std::io::ErrorKind::WouldBlock { return Err(format!("lock l3 approvals: {}", std::io::Error::last_os_error())); }
                }
                #[cfg(not(unix))]
                { return Ok(StoreLock { file }); }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(e) => return Err(format!("lock l3 approvals: {e}")),
        }
    }
    Err("lock l3 approvals: timed out".to_string())
}

/// Persist `queue` to `path` (creates parent dirs; atomic replace).
pub fn save_queue(path: &Path, queue: &ApprovalQueue) -> Result<(), String> {
    let _lock = acquire_lock(path)?;
    save_queue_unlocked(path, queue)
}

fn save_queue_unlocked(path: &Path, queue: &ApprovalQueue) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create l3 approvals directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&queue_to_wire(queue)).map_err(|e| e.to_string())?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or(STORE_FILE);
    let temp_path = path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), unique_suffix()));
    use std::os::unix::fs::PermissionsExt;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp_path)
        .map_err(|e| format!("save l3 approvals temp file: {e}"))?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("set l3 approvals permissions: {e}"))?;
    std::io::Write::write_all(&mut file, json.as_bytes()).map_err(|e| format!("save l3 approvals temp file: {e}"))?;
    drop(file);
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!("replace l3 approvals: {e}")
    })
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// Reload from disk, run `f`, save. Used by MCP + desktop so both see the same pending set.
pub fn with_queue<R>(path: &Path, f: impl FnOnce(&mut ApprovalQueue) -> R) -> Result<R, String> {
    let _lock = acquire_lock(path)?;
    let mut q = load_queue(path)?;
    let out = f(&mut q);
    save_queue_unlocked(path, &q)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_agents::approval::{ConfirmIntent, Decision};

    fn tmp() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("shogun-l3-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        p
    }

    #[test]
    fn round_trip_preserves_pending_send() {
        let path = tmp();
        let send = SendAction::SendEmail { to: "a@b.com".into() };
        let preview = Preview::for_send(&send, "Subject: Hi\n\nbody", Route::ViaComposio);
        let id = with_queue(&path, |q| q.request(send.clone(), preview.clone(), Origin::AiApi, 42)).unwrap();

        let loaded = load_queue(&path).unwrap();
        assert_eq!(loaded.pending_len(), 1);
        assert_eq!(loaded.origin(id), Some(Origin::AiApi));
        assert_eq!(loaded.preview(id).map(|p| p.full_body.as_str()), Some("Subject: Hi\n\nbody"));
        assert!(matches!(
            {
                let mut q = loaded;
                q.confirm(id, ConfirmIntent::DedicatedButton, 100)
            },
            Decision::Confirmed(cs) if cs.action == send && cs.preview.route == Route::ViaComposio
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_empty_queue() {
        let path = tmp();
        assert_eq!(load_queue(&path).unwrap().pending_len(), 0);
    }

    #[test]
    fn corrupt_file_is_error_not_empty_queue() {
        let path = tmp();
        std::fs::write(&path, b"not json").unwrap();
        assert!(load_queue(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn terminal_status_drops_full_body_and_survives_restart() {
        let path = tmp();
        let id = with_queue(&path, |q| {
            let send = SendAction::SendEmail { to: "a@b.com".into() };
            q.request(send.clone(), Preview::for_send(&send, "SECRET BODY", Route::ViaComposio), Origin::AiApi, 1)
        }).unwrap();
        with_queue(&path, |q| { let _ = q.confirm(id, ConfirmIntent::DedicatedButton, 2); }).unwrap();
        with_queue(&path, |q| assert!(q.mark_status(id, ApprovalStatus::Sent, 2))).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("SECRET BODY"));
        assert_eq!(load_queue(&path).unwrap().status(id), Some(ApprovalStatus::Sent));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resolve_defaults_next_to_db() {
        assert_eq!(
            resolve_store_path("/Users/x/Library/Application Support/com.selectkk.shogun/memory.db"),
            PathBuf::from("/Users/x/Library/Application Support/com.selectkk.shogun/l3_approvals.json")
        );
        assert_eq!(resolve_store_path("memory.db"), PathBuf::from(STORE_FILE));
    }

    #[test]
    fn semantically_invalid_rows_fail_closed() {
        let path = tmp();
        std::fs::write(&path, r#"{"next_id":2,"pending":[{"id":1,"kind":"send_email","destination":"a@b.com","full_body":"x","route":"direct","origin":"ai_api","created_ms":1}]}"#).unwrap();
        let error = match load_queue(&path) { Ok(_) => panic!("invalid row accepted"), Err(error) => error };
        assert!(error.contains("route does not match"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_terminal_status_fails_closed() {
        let path = tmp();
        std::fs::write(&path, r#"{"next_id":2,"terminal":[{"id":1,"status":"pending","resolved_ms":1}]}"#).unwrap();
        assert!(load_queue(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn existing_unlocked_lock_file_does_not_block_transaction() {
        let path = tmp();
        let lock = lock_path(&path);
        std::fs::File::create(&lock).unwrap();
        let send = SendAction::SendEmail { to: "a@b.com".into() };
        assert!(with_queue(&path, |q| q.request(send.clone(), Preview::for_send(&send, "body", Route::ViaComposio), Origin::AiApi, 1)).is_ok());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&lock);
    }

    #[test]
    fn concurrent_transactions_serialize_exclusively() {
        let path = std::sync::Arc::new(tmp());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for i in 0..2 {
            let path = path.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let send = SendAction::SendEmail { to: format!("{i}@b.com") };
                with_queue(&path, |q| q.request(send.clone(), Preview::for_send(&send, "body", Route::ViaComposio), Origin::AiApi, i)).unwrap()
            }));
        }
        let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(load_queue(&path).unwrap().pending_len(), 2);
        let _ = std::fs::remove_file(&*path);
    }
}
