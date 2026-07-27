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
pub mod mac {
    use std::sync::Mutex;

    use serde_json::json;
    use shogun_agents::approval::{ApprovalId, ApprovalQueue, ConfirmIntent, Decision, Origin};
    use shogun_agents::permission::SendAction;
    use shogun_agents::producer::{propose, ProposedSend};
    use shogun_core::composio_send::HttpComposioApi;
    use shogun_core::daemon::Db;
    use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
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

    fn route_str(route: shogun_agents::approval::Route) -> &'static str {
        match route {
            shogun_agents::approval::Route::DirectMcp => "direct",
            shogun_agents::approval::Route::ViaComposio => "composio",
        }
    }

    /// Build a [`ProposedSend`] from UI/agent input. Email → Composio (§6.10); everything else →
    /// direct first-layer. The action/preview/route construction is centralized in
    /// `shogun_agents::producer`.
    fn proposed(kind: &str, destination: &str, subject: &str, body: &str) -> Result<ProposedSend, String> {
        Ok(match kind {
            "email" => ProposedSend::Email {
                to: destination.to_string(),
                subject: subject.to_string(),
                body: body.to_string(),
            },
            "slack" => ProposedSend::SlackPost { channel: destination.to_string(), body: body.to_string() },
            "calendar" => ProposedSend::CalendarEvent { title: destination.to_string(), body: body.to_string() },
            "github" => ProposedSend::IssueComment { target: destination.to_string(), body: body.to_string() },
            other => return Err(format!("unknown send kind: {other}")),
        })
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
        let proposal = proposed(&kind, &destination, &subject, &body)?;
        let now = db.now_ms().max(0) as u64;
        let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
        Ok(propose(&mut q, &proposal, Origin::Human, now).0)
    }

    /// Reply Drafter (FR-AG-10) and the other draft-then-send agents: draft the body on the BYOK
    /// Agent lane (invariant 5) from the given context, then enqueue it as an L3 proposal. `kind`
    /// selects the send type (email/slack/calendar/github); `destination` is the recipient/channel/
    /// title/target; `context` is the thread/screen text the draft is grounded in. Returns the
    /// pending approval id. The human still confirms before anything sends (FR-AG-03).
    #[tauri::command]
    pub fn draft_reply(
        kind: String,
        destination: String,
        subject: String,
        context: String,
        state: tauri::State<'_, ApprovalQueueState>,
        db: tauri::State<'_, Db>,
    ) -> Result<u64, String> {
        use shogun_core::llm::AgentClient;
        let prompt = format!(
            "You are drafting a concise, professional {kind} reply. Use the context below; write \
             only the reply body, no preamble.\n\n--- context ---\n{context}"
        );
        // Draft through the same BYOK Agent-lane client as inline drafts (invariant 5). Traceability
        // is recorded by the client at the egress point.
        let agent = crate::inline_source::mac::build_agent(&db)
            .ok_or_else(|| "No key yet — add your provider key in Settings to draft replies.".to_string())?;
        let body = agent.complete(&prompt).map_err(|e| format!("draft failed: {e:?}"))?;

        let proposal = proposed(&kind, &destination, &subject, &body)?;
        let now = db.now_ms().max(0) as u64;
        let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
        Ok(propose(&mut q, &proposal, Origin::Human, now).0)
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
        let composio_key = composio_api_key()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| "Composio key not set — add it in settings to send".to_string())?;
        let composio_user = std::env::var("SHOGUN_COMPOSIO_USER_ID").unwrap_or_default();
        let composio = ComposioSendTransport::new(HttpComposioApi::new(composio_key)?, composio_user);
        let runtime = connectors.0.clone();
        let first_layer = FirstLayerSendTransport::new(&connectors.0);
        let draft_runtime = runtime.clone();
        let draft_sink = db.traceability_sink();
        let routed = RoutedSendTransport::new(
            composio,
            first_layer,
            Box::new(move |action, body| save_gmail_draft(&draft_runtime, &draft_sink, action, body)),
        );

        match execute_send(&confirmed, &routed, &db.traceability_sink()) {
            SendExecOutcome::Sent => Ok("sent".into()),
            SendExecOutcome::Failed(e) => Ok(format!("failed:{e}")),
        }
    }

    /// FR-C2-05 fallback: save a Gmail draft (first-layer L2) when a Composio send fails.
    /// Records traceability (invariant 3) only on a successful egress; a failed write traces nothing
    /// (mirrors `execute_send` — nothing left the device, so nothing is recorded).
    fn save_gmail_draft(
        runtime: &std::sync::Arc<Mutex<Runtime>>,
        sink: &impl TraceabilitySink,
        action: &SendAction,
        body: &str,
    ) -> Result<(), String> {
        let SendAction::SendEmail { to } = action else {
            return Err("draft fallback only applies to email".into());
        };
        let (subject, mail_body) = shogun_mcp::composio::parse_gmail_full_body(body);
        // Key names are the arg contract of `create_draft` → `gmail_shape::draft_request_body`,
        // which reads `to`/`subject`/`body`. The old official-MCP tool took `recipient_email`; after
        // the transport swap to GmailRestRpc that name silently produced "draft: missing to" and the
        // FR-C2-05 fallback never wrote a draft. `gmail_shape::draft_request_body` has a matching test.
        let args = json!({ "to": to, "subject": subject, "body": mail_body });
        let rt = runtime.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        rt.execute_write_owned(shogun_mcp::scope::Service::Gmail, "draft_create_update", args).map(|_| ())?;
        // Record traceability only on success: the draft body just left the device to Google via
        // GmailRestRpc. Route::Mcp = first-layer direct-to-Google; third_party = false (Composio
        // is the third-party arm; this fallback bypasses it). Chunk is digested and dropped — body
        // text never reaches storage (G8 / invariant 3).
        sink.record(TraceRecord::for_chunk(
            Route::Mcp,
            "draft_create",
            "gmail",
            &mail_body,
            false,
        ));
        Ok(())
    }

    /// The Composio API key is a plain secret (not a TokenSet) — read it directly from the Keychain.
    fn composio_api_key() -> Option<String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, COMPOSIO_KEY_ACCOUNT)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    }
}
