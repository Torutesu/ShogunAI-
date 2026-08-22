//! Durable, cross-process L3 approval queue. Store data is local, 0600, and contains full
//! previews only until a human resolves the request; terminal rows are deliberately body-free.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::time::Duration;

use shogun_agents::approval::{
    ApprovalId, ApprovalOrigin, ApprovalQueue, ApprovalStatus, PendingRecord, Preview, Route,
    TerminalRecord,
};
use shogun_agents::permission::SendAction;

pub const STORE_FILE: &str = "l3_approvals.json";
pub const STORE_ENV: &str = "SHOGUN_L3_APPROVALS";
const MAX_PENDING: usize = 64;
const MAX_TERMINAL: usize = 256;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_DESTINATION_BYTES: usize = 8192;
const MAX_STORE_BYTES: u64 = 18 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WireStore {
    schema: u8,
    next_id: u64,
    pending: Vec<WirePending>,
    terminal: Vec<WireTerminal>,
    in_flight: Vec<u64>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
struct WireTerminal {
    id: u64,
    status: String,
    resolved_ms: u64,
}

pub fn resolve_store_path(db_path: &str) -> PathBuf {
    if let Ok(path) = std::env::var(STORE_ENV) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    Path::new(db_path)
        .parent()
        .map(|dir| dir.join(STORE_FILE))
        .unwrap_or_else(|| PathBuf::from(STORE_FILE))
}

fn route_wire(route: Route) -> &'static str {
    match route {
        Route::DirectMcp => "direct",
        Route::ViaComposio => "composio",
    }
}
fn route_parse(value: &str) -> Result<Route, String> {
    match value {
        "direct" => Ok(Route::DirectMcp),
        "composio" => Ok(Route::ViaComposio),
        _ => Err("invalid approval route".into()),
    }
}
fn origin_wire(origin: ApprovalOrigin) -> &'static str {
    origin.as_str()
}
fn origin_parse(value: &str) -> Result<ApprovalOrigin, String> {
    match value {
        "ui" => Ok(ApprovalOrigin::Ui),
        "api" => Ok(ApprovalOrigin::Api),
        "mcp" => Ok(ApprovalOrigin::Mcp),
        _ => Err("invalid approval origin".into()),
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
fn status_parse(value: &str) -> Result<ApprovalStatus, String> {
    match value {
        "rejected" => Ok(ApprovalStatus::Rejected),
        "timed_out" => Ok(ApprovalStatus::TimedOut),
        "sent" => Ok(ApprovalStatus::Sent),
        "send_failed" => Ok(ApprovalStatus::SendFailed),
        "draft_saved" => Ok(ApprovalStatus::DraftSaved),
        _ => Err("invalid terminal approval status".into()),
    }
}

fn action_kind(action: &SendAction) -> &'static str {
    match action {
        SendAction::SendEmail { .. } => "send_email",
        SendAction::PostMessage { .. } => "post_message",
        SendAction::AddReaction { .. } => "add_reaction",
        SendAction::CreateCalendarEvent { .. } => "create_calendar_event",
        SendAction::UpdateCalendarEvent { .. } => "update_calendar_event",
        SendAction::PostComment { .. } => "post_comment",
        SendAction::CreateDocument { .. } => "create_document",
        SendAction::UpdateDocument { .. } => "update_document",
        SendAction::ChangeIssueStatus { .. } => "change_issue_status",
    }
}
fn action_from_wire(kind: &str, destination: String) -> Result<SendAction, String> {
    if destination.trim().is_empty() || destination.len() > MAX_DESTINATION_BYTES {
        return Err("invalid approval destination".into());
    }
    Ok(match kind {
        "send_email" => SendAction::SendEmail { to: destination },
        "post_message" => SendAction::PostMessage {
            channel: destination,
        },
        "add_reaction" => SendAction::AddReaction {
            target: destination,
        },
        "create_calendar_event" => SendAction::CreateCalendarEvent { title: destination },
        "update_calendar_event" => SendAction::UpdateCalendarEvent { title: destination },
        "post_comment" => SendAction::PostComment {
            target: destination,
        },
        "create_document" => SendAction::CreateDocument { title: destination },
        "update_document" => SendAction::UpdateDocument { title: destination },
        "change_issue_status" => SendAction::ChangeIssueStatus {
            target: destination,
        },
        _ => return Err("invalid approval action kind".into()),
    })
}
fn expected_route(action: &SendAction) -> Route {
    if matches!(action, SendAction::SendEmail { .. }) {
        Route::ViaComposio
    } else {
        Route::DirectMcp
    }
}
fn pending_to_wire(record: &PendingRecord) -> WirePending {
    WirePending {
        id: record.id.0,
        kind: action_kind(&record.action).into(),
        destination: record.preview.destination.clone(),
        full_body: record.preview.full_body.clone(),
        route: route_wire(record.preview.route).into(),
        origin: origin_wire(record.origin).into(),
        created_ms: record.created_ms,
    }
}
fn pending_from_wire(row: WirePending) -> Result<PendingRecord, String> {
    if row.id == 0 || row.full_body.len() > MAX_BODY_BYTES {
        return Err("invalid approval preview".into());
    }
    let action = action_from_wire(&row.kind, row.destination.clone())?;
    let route = route_parse(&row.route)?;
    if route != expected_route(&action) {
        return Err("approval route does not match action".into());
    }
    Ok(PendingRecord {
        id: ApprovalId(row.id),
        action: action.clone(),
        preview: Preview::for_send(&action, row.full_body, route),
        origin: origin_parse(&row.origin)?,
        created_ms: row.created_ms,
    })
}
fn queue_to_wire(queue: &ApprovalQueue) -> WireStore {
    let (next_id, pending) = queue.export();
    WireStore {
        schema: 1,
        next_id,
        pending: pending.iter().map(pending_to_wire).collect(),
        terminal: queue
            .terminal_records()
            .iter()
            .map(|row| WireTerminal {
                id: row.id.0,
                status: status_wire(row.status).into(),
                resolved_ms: row.resolved_ms,
            })
            .collect(),
        in_flight: queue.in_flight_ids().iter().map(|id| id.0).collect(),
    }
}
fn queue_from_wire(store: WireStore) -> Result<ApprovalQueue, String> {
    if store.schema != 1
        || store.next_id == 0
        || store.pending.len() > MAX_PENDING
        || store.terminal.len() > MAX_TERMINAL
    {
        return Err("invalid approval store limits".into());
    }
    let mut ids = HashSet::new();
    let mut pending = Vec::with_capacity(store.pending.len());
    for row in store.pending {
        if !ids.insert(row.id) {
            return Err("duplicate approval id".into());
        }
        pending.push(pending_from_wire(row)?);
    }
    let mut terminal = Vec::with_capacity(store.terminal.len());
    for row in store.terminal {
        if row.id == 0 || !ids.insert(row.id) {
            return Err("duplicate approval id".into());
        }
        terminal.push(TerminalRecord {
            id: ApprovalId(row.id),
            status: status_parse(&row.status)?,
            resolved_ms: row.resolved_ms,
        });
    }
    let mut in_flight = Vec::with_capacity(store.in_flight.len());
    for id in store.in_flight {
        if id == 0 || !ids.insert(id) {
            return Err("duplicate approval id".into());
        }
        in_flight.push(ApprovalId(id));
    }
    Ok(ApprovalQueue::import_with_terminal(
        store.next_id,
        pending,
        terminal,
        in_flight,
    ))
}

pub fn load_queue(path: &Path) -> Result<ApprovalQueue, String> {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_STORE_BYTES => return Err("approval store too large".into()),
        #[cfg(unix)]
        Ok(meta) if std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o077 != 0 => {
            return Err("approval store permissions are not private".into());
        }
        Ok(_) | Err(_) => {}
    }
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|_| "invalid approval store".to_string())
            .and_then(queue_from_wire),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ApprovalQueue::new()),
        Err(_) => Err("cannot read approval store".into()),
    }
}

// The handle is only *read* on unix (flock/unlock); on other platforms it exists to hold the
// file open for the lock file's lifetime.
struct StoreLock(#[cfg_attr(not(unix), allow(dead_code))] File);
impl Drop for StoreLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.0), libc::LOCK_UN);
        }
    }
}
fn lock_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.lock",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(STORE_FILE)
    ))
}
fn acquire_lock(path: &Path) -> Result<StoreLock, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "cannot create approval store directory".to_string())?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path(path))
        .map_err(|_| "cannot lock approval store".to_string())?;
    #[cfg(unix)]
    for _ in 0..500 {
        if unsafe {
            libc::flock(
                std::os::fd::AsRawFd::as_raw_fd(&lock),
                libc::LOCK_EX | libc::LOCK_NB,
            )
        } == 0
        {
            return Ok(StoreLock(lock));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    #[cfg(not(unix))]
    return Ok(StoreLock(lock));
    #[cfg(unix)]
    Err("approval store lock timed out".into())
}
fn save_unlocked(path: &Path, queue: &ApprovalQueue) -> Result<(), String> {
    let data = serde_json::to_vec(&queue_to_wire(queue))
        .map_err(|_| "cannot encode approval store".to_string())?;
    if data.len() > MAX_STORE_BYTES as usize {
        return Err("approval store too large".into());
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(STORE_FILE);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock error".to_string())?
        .as_nanos();
    let temp = path.with_file_name(format!(".{filename}.{}.{}.tmp", std::process::id(), nonce));
    #[cfg(unix)]
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    #[cfg(unix)]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "cannot write approval store".to_string())?;
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|_| "cannot write approval store".to_string())?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot protect approval store".to_string())?;
    file.write_all(&data)
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot persist approval store".to_string())?;
    fs::rename(&temp, path).map_err(|_| "cannot replace approval store".to_string())?;
    // Directory fsync is a unix durability step; Windows cannot File::open a directory at all,
    // so this must not run there (it would fail every save).
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| "cannot sync approval store".to_string())?;
    }
    Ok(())
}
/// Why [`validate_enqueue`] refused a send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueRefusal {
    /// The request itself is malformed (empty or oversized destination, oversized body).
    Invalid(&'static str),
    /// The store already holds `MAX_PENDING` unresolved sends; retry after some resolve.
    QueueFull,
}

/// Enqueue-side twin of the load-time row checks. `load_queue` rejects a store whose rows break
/// these limits, so an enqueue that skips them persists a file every later load refuses — one bad
/// request would brick the whole queue for every face. Callers must validate before `try_request`.
pub fn validate_enqueue(queue: &ApprovalQueue, preview: &Preview) -> Result<(), EnqueueRefusal> {
    if preview.destination.trim().is_empty() || preview.destination.len() > MAX_DESTINATION_BYTES {
        return Err(EnqueueRefusal::Invalid("invalid approval destination"));
    }
    if preview.full_body.len() > MAX_BODY_BYTES {
        return Err(EnqueueRefusal::Invalid("approval body too large"));
    }
    if queue.pending_len() >= MAX_PENDING {
        return Err(EnqueueRefusal::QueueFull);
    }
    Ok(())
}

pub fn with_queue<R>(
    path: &Path,
    change: impl FnOnce(&mut ApprovalQueue) -> R,
) -> Result<R, String> {
    let _lock = acquire_lock(path)?;
    let mut queue = load_queue(path)?;
    let output = change(&mut queue);
    save_unlocked(path, &queue)?;
    Ok(output)
}

/// Resolve work left in flight by a previous desktop process.
///
/// This is deliberately separate from [`with_queue`]. An in-flight row can belong to a send that
/// is still running outside the file lock, so ordinary reads, polls, and post-send writes must not
/// mistake live work for a crash. The desktop calls this once when it becomes the queue executor.
pub fn recover_in_flight(path: &Path, recovered_ms: u64) -> Result<Vec<ApprovalId>, String> {
    let _lock = acquire_lock(path)?;
    let mut queue = load_queue(path)?;
    let recovered = queue.recover_in_flight(recovered_ms);
    save_unlocked(path, &queue)?;
    Ok(recovered)
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "shogun-l3-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn persists_pending_and_body_free_terminal() {
        let path = path();
        let send = SendAction::SendEmail {
            to: "a@b.com".into(),
        };
        let id = with_queue(&path, |q| {
            q.try_request(
                send.clone(),
                Preview::for_send(&send, "private", Route::ViaComposio),
                ApprovalOrigin::Mcp,
                1,
            )
        })
        .unwrap()
        .unwrap();
        with_queue(&path, |q| {
            assert!(matches!(
                q.confirm(
                    id,
                    shogun_agents::approval::ConfirmIntent::DedicatedButton,
                    2
                ),
                shogun_agents::approval::Decision::Confirmed(_)
            ));
            assert!(q.mark_status(id, ApprovalStatus::Sent, 3));
        })
        .unwrap();
        assert_eq!(
            load_queue(&path).unwrap().status(id),
            Some(ApprovalStatus::Sent)
        );
        assert!(!fs::read_to_string(&path).unwrap().contains("private"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn polling_live_in_flight_work_cannot_force_failure() {
        let path = path();
        let send = SendAction::PostMessage {
            channel: "x".into(),
        };
        let id = with_queue(&path, |q| {
            q.try_request(
                send.clone(),
                Preview::for_send(&send, "private", Route::DirectMcp),
                ApprovalOrigin::Api,
                1,
            )
        })
        .unwrap()
        .unwrap();

        with_queue(&path, |q| {
            assert!(matches!(
                q.confirm(
                    id,
                    shogun_agents::approval::ConfirmIntent::DedicatedButton,
                    2
                ),
                shogun_agents::approval::Decision::Confirmed(_)
            ));
        })
        .unwrap();

        with_queue(&path, |q| {
            assert_eq!(q.status(id), Some(ApprovalStatus::Pending));
        })
        .unwrap();

        with_queue(&path, |q| {
            assert!(q.mark_status(id, ApprovalStatus::Sent, 3));
        })
        .unwrap();
        assert_eq!(
            load_queue(&path).unwrap().status(id),
            Some(ApprovalStatus::Sent)
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn rejects_invalid_or_duplicate_wire() {
        let path = path();
        fs::write(&path, r#"{"schema":1,"next_id":2,"pending":[{"id":1,"kind":"send_email","destination":"a","full_body":"x","route":"direct","origin":"mcp","created_ms":1}],"terminal":[],"in_flight":[]}"#).unwrap();
        assert!(load_queue(&path).is_err());
        let _ = fs::remove_file(path);
    }
    #[test]
    fn recovers_in_flight_as_failure() {
        let path = path();
        let send = SendAction::PostMessage {
            channel: "x".into(),
        };
        let id = with_queue(&path, |q| {
            let id = q
                .try_request(
                    send.clone(),
                    Preview::for_send(&send, "x", Route::DirectMcp),
                    ApprovalOrigin::Api,
                    1,
                )
                .unwrap();
            let _ = q.confirm(
                id,
                shogun_agents::approval::ConfirmIntent::DedicatedButton,
                2,
            );
            id
        })
        .unwrap();
        assert_eq!(recover_in_flight(&path, 3).unwrap(), vec![id]);
        assert_eq!(
            load_queue(&path).unwrap().status(id),
            Some(ApprovalStatus::SendFailed)
        );
        let _ = fs::remove_file(path);
    }
}
