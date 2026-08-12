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

use shogun_agents::approval::{
    ApprovalId, ApprovalQueue, KeyKind, Origin, PendingRecord, Preview, Route,
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

impl Default for WireStore {
    fn default() -> Self {
        Self { next_id: 1, pending: Vec::new() }
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

fn route_parse(s: &str) -> Option<Route> {
    match s {
        "direct" => Some(Route::DirectMcp),
        "composio" => Some(Route::ViaComposio),
        _ => None,
    }
}

fn origin_wire(origin: Origin) -> &'static str {
    match origin {
        Origin::Human => "human",
        Origin::AiApi => "ai_api",
    }
}

fn origin_parse(s: &str) -> Option<Origin> {
    match s {
        "human" => Some(Origin::Human),
        "ai_api" => Some(Origin::AiApi),
        _ => None,
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

fn action_from_wire(kind: &str, destination: String) -> Option<SendAction> {
    Some(match kind {
        "send_email" => SendAction::SendEmail { to: destination },
        "post_message" => SendAction::PostMessage { channel: destination },
        "create_calendar_event" => SendAction::CreateCalendarEvent { title: destination },
        "post_comment" => SendAction::PostComment { target: destination },
        _ => return None,
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

fn from_wire(w: WirePending) -> Option<PendingRecord> {
    let action = action_from_wire(&w.kind, w.destination.clone())?;
    let route = route_parse(&w.route)?;
    let origin = origin_parse(&w.origin)?;
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
    Some(PendingRecord {
        id: ApprovalId(w.id),
        action,
        preview,
        origin,
        created_ms: w.created_ms,
    })
}

fn queue_to_wire(q: &ApprovalQueue) -> WireStore {
    let (next_id, records) = q.export();
    WireStore { next_id, pending: records.iter().map(to_wire).collect() }
}

fn queue_from_wire(store: WireStore) -> ApprovalQueue {
    let records: Vec<PendingRecord> = store.pending.into_iter().filter_map(from_wire).collect();
    ApprovalQueue::import(store.next_id, records)
}

/// Load the shared queue from `path`. Missing/unreadable → empty queue.
pub fn load_queue(path: &Path) -> ApprovalQueue {
    let store = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<WireStore>(&text).ok())
        .unwrap_or_default();
    queue_from_wire(store)
}

/// Persist `queue` to `path` (creates parent dirs; atomic replace).
pub fn save_queue(path: &Path, queue: &ApprovalQueue) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create l3 approvals directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&queue_to_wire(queue)).map_err(|e| e.to_string())?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or(STORE_FILE);
    let temp_path = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&temp_path, json).map_err(|e| format!("save l3 approvals temp file: {e}"))?;
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!("replace l3 approvals: {e}")
    })
}

/// Reload from disk, run `f`, save. Used by MCP + desktop so both see the same pending set.
pub fn with_queue<R>(path: &Path, f: impl FnOnce(&mut ApprovalQueue) -> R) -> Result<R, String> {
    let mut q = load_queue(path);
    let out = f(&mut q);
    save_queue(path, &q)?;
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

        let loaded = load_queue(&path);
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
        assert_eq!(load_queue(&path).pending_len(), 0);
    }

    #[test]
    fn resolve_defaults_next_to_db() {
        assert_eq!(
            resolve_store_path("/Users/x/Library/Application Support/com.selectkk.shogun/memory.db"),
            PathBuf::from("/Users/x/Library/Application Support/com.selectkk.shogun/l3_approvals.json")
        );
        assert_eq!(resolve_store_path("memory.db"), PathBuf::from(STORE_FILE));
    }
}
