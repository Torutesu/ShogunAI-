//! L3 approval queue → send execution (item B, §6.6 / §6.14). ROUGH / macOS-only.
//!
//! Wires the shared [`ApprovalQueue`] into the running app: a producer enqueues a send
//! (`submit_send` — an agent, or a manual test), the UI lists pending L3 confirmations, and on a
//! dedicated-button confirm the send actually executes through
//! [`shogun_core::send_exec::RoutedSendTransport`] (first-layer MCP for posts/events/comments,
//! Composio for Gmail send) with mandatory traceability. Enter-key alone never confirms (FR-AG-03);
//! the 10-minute timeout is enforced (FR-AG-04).
//!
//! Cannot compile on Linux (Keychain + network); the CI macOS job verifies it.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub use mac::ApprovalQueueState;

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;

    use serde_json::json;
    use shogun_agents::approval::{
        ApprovalId, ApprovalQueue, ConfirmIntent, Decision, Origin, Preview, Route,
    };
    use shogun_agents::permission::SendAction;
    use shogun_core::composio_send::HttpComposioApi;
    use shogun_core::daemon::Db;
    use shogun_core::send_exec::{
        execute_send, ComposioSendTransport, FirstLayerSendTransport, RoutedSendTransport,
        SendExecOutcome,
    };

    use crate::connectors::mac::{ConnectorState, Runtime};

    const KEYCHAIN_SERVICE: &str = "com.selectkk.shogun";
    const COMPOSIO_KEY_ACCOUNT: &str = "composio-api-key";

    /// The shared L3 approval queue (the same one an agent enqueues into and the UI drains).
    pub struct ApprovalQueueState(pub Mutex<ApprovalQueue>);

    impl Default for ApprovalQueueState {
        fn default() -> Self {
            Self(Mutex::new(ApprovalQueue::new()))
        }
    }

    /// One row for the L3 confirm UI (FR-AG-03: op type, full destination, full body, route).
    #[derive(serde::Serialize)]
    pub struct ApprovalView {
        pub id: u64,
        pub op_type: &'static str,
        pub destination: String,
        pub full_body: String,
        pub route: &'static str,
    }

    fn route_str(route: Route) -> &'static str {
        match route {
            Route::DirectMcp => "direct",
            Route::ViaComposio => "composio",
        }
    }

    /// Build a [`SendAction`] + [`Preview`] from UI/agent input. Email routes ViaComposio (§6.10);
    /// everything else is a direct first-layer send. Email `full_body` matches the
    /// `prepare_send` "Subject: …\n\n…" shape so the executor can split it back.
    fn build_send(kind: &str, destination: &str, subject: &str, body: &str) -> Result<(SendAction, Preview), String> {
        let (action, full, route) = match kind {
            "email" => (
                SendAction::SendEmail { to: destination.to_string() },
                format!("Subject: {subject}\n\n{body}"),
                Route::ViaComposio,
            ),
            "slack" => (SendAction::PostMessage { channel: destination.to_string() }, body.to_string(), Route::DirectMcp),
            "calendar" => (SendAction::CreateCalendarEvent { title: destination.to_string() }, body.to_string(), Route::DirectMcp),
            "github" => (SendAction::PostComment { target: destination.to_string() }, body.to_string(), Route::DirectMcp),
            other => return Err(format!("unknown send kind: {other}")),
        };
        let preview = Preview::for_send(&action, full, route);
        Ok((action, preview))
    }

    /// Enqueue a send for L3 confirmation. Returns the pending id. The producer is normally an agent
    /// (Reply Drafter etc.); this command is the shared entry point (also usable from the UI).
    #[tauri::command]
    pub fn submit_send(
        kind: String,
        destination: String,
        subject: String,
        body: String,
        state: tauri::State<'_, ApprovalQueueState>,
        db: tauri::State<'_, Db>,
    ) -> Result<u64, String> {
        let (action, preview) = build_send(&kind, &destination, &subject, &body)?;
        let now = db.now_ms().max(0) as u64;
        let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
        Ok(q.request(action, preview, Origin::Human, now).0)
    }

    /// List pending L3 confirmations (expiring any past the 10-minute window first).
    #[tauri::command]
    pub fn list_approvals(
        state: tauri::State<'_, ApprovalQueueState>,
        db: tauri::State<'_, Db>,
    ) -> Result<Vec<ApprovalView>, String> {
        let now = db.now_ms().max(0) as u64;
        let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
        q.expire_due(now);
        let views = q
            .pending_ids()
            .into_iter()
            .filter_map(|id| q.preview(id).map(|p| (id, p)))
            .map(|(id, p)| ApprovalView {
                id: id.0,
                op_type: p.op_type,
                destination: p.destination.clone(),
                full_body: p.full_body.clone(),
                route: route_str(p.route),
            })
            .collect();
        Ok(views)
    }

    /// Reject a pending send.
    #[tauri::command]
    pub fn reject_send(id: u64, state: tauri::State<'_, ApprovalQueueState>) -> Result<String, String> {
        let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
        use shogun_agents::approval::RejectCause;
        match q.reject(ApprovalId(id), RejectCause::UserRejected) {
            Decision::Rejected(_) => Ok("rejected".into()),
            other => Ok(format!("{other:?}")),
        }
    }

    /// Confirm a pending send via the dedicated button (FR-AG-03) and execute it. Enter-key intent
    /// must be sent as a separate flag by the UI; this command is the button path.
    #[tauri::command]
    pub fn confirm_send(
        id: u64,
        state: tauri::State<'_, ApprovalQueueState>,
        connectors: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
    ) -> Result<String, String> {
        let now = db.now_ms().max(0) as u64;
        // Confirm + dequeue under the queue lock, then drop it before executing (execution locks the
        // connector runtime, a different lock — keep the two lock scopes disjoint).
        let confirmed = {
            let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
            match q.confirm(ApprovalId(id), ConfirmIntent::DedicatedButton, now) {
                Decision::Confirmed(cs) => cs,
                Decision::RequiresDedicatedButton => return Ok("requires_button".into()),
                Decision::StillPending => return Ok("pending".into()),
                Decision::Rejected(c) => return Ok(format!("rejected:{c:?}")),
                Decision::Unknown => return Ok("unknown".into()),
            }
        };

        // Build the routed transport: Composio for email, first-layer MCP for the rest, with the
        // FR-C2-05 draft fallback (save a Gmail draft if Composio fails).
        let composio_key = composio_api_key().unwrap_or_default();
        let composio_user = std::env::var("SHOGUN_COMPOSIO_USER_ID").unwrap_or_default();
        let composio = ComposioSendTransport::new(HttpComposioApi::new(composio_key)?, composio_user);
        let runtime = connectors.0.clone();
        let first_layer = FirstLayerSendTransport::new(&connectors.0);
        let draft_runtime = runtime.clone();
        let routed = RoutedSendTransport::new(
            composio,
            first_layer,
            Box::new(move |action, body| save_gmail_draft(&draft_runtime, action, body)),
        );

        match execute_send(&confirmed, &routed, &db.traceability_sink()) {
            SendExecOutcome::Sent => Ok("sent".into()),
            SendExecOutcome::Failed(e) => Ok(format!("failed:{e}")),
        }
    }

    /// FR-C2-05 fallback: save a Gmail draft (first-layer L2) when a Composio send fails.
    fn save_gmail_draft(
        runtime: &std::sync::Arc<Mutex<Runtime>>,
        action: &SendAction,
        body: &str,
    ) -> Result<(), String> {
        let SendAction::SendEmail { to } = action else {
            return Err("draft fallback only applies to email".into());
        };
        let (subject, mail_body) = shogun_mcp::composio::parse_gmail_full_body(body);
        let args = json!({ "recipient_email": to, "subject": subject, "body": mail_body });
        let rt = runtime.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        rt.execute_write_owned(shogun_mcp::scope::Service::Gmail, "draft_create_update", args).map(|_| ())
    }

    /// The Composio API key is a plain secret (not a TokenSet) — read it directly from the Keychain.
    fn composio_api_key() -> Option<String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, COMPOSIO_KEY_ACCOUNT)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    }
}
