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
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use shogun_agents::approval::{
        ApprovalId, ApprovalOrigin, ApprovalQueue, ConfirmIntent, ConfirmedSend, Decision, Preview,
    };
    use shogun_agents::permission::SendAction;
    use shogun_memory::lessons::{FeedbackKind, LessonScope, NewFeedback};
    use shogun_agents::producer::{propose, ProposedSend};
    use shogun_core::composio_send::HttpComposioApi;
    use shogun_core::daemon::Db;
    use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
    use shogun_core::send_exec::{
        execute_send, ComposioSendTransport, FirstLayerSendTransport, RoutedSendTransport,
        SendExecOutcome,
    };

    use crate::connectors::mac::{ConnectorState, Runtime};

    use shogun_integrations::keychain_store;

    const COMPOSIO_KEY_ACCOUNT: &str = "composio-api-key";

    // ---- Composio policy (non-secret: stored in JSON, NOT the Keychain) --------------------

    /// Persisted opt-in policy for the Composio second-layer send (FR-C2-02 / FR-C2-03).
    /// Both fields default to the safe-blocked state: no send can happen without the user
    /// deliberately enabling each gate. Stored at `<app-data>/composio.json`; absent/unreadable →
    /// `Default` (both gates closed). Secrets (the API key) stay in the Keychain (invariant 7).
    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    pub(crate) struct ComposioPolicy {
        /// When `true` (default) the send path is blocked even after consent — the user can draft
        /// only. The gate must be explicitly turned OFF to allow a live send (FR-C2-03).
        pub draft_stop: bool,
        /// Whether the user has completed the FR-C2-02 opt-in disclosure screen. `false` by
        /// default — no send is ever attempted without an explicit acknowledgement.
        pub consent_acknowledged: bool,
        /// Composio account user ID for the connected Gmail account. Falls back to the
        /// `SHOGUN_COMPOSIO_USER_ID` env var when empty. Stored in policy JSON (not a secret).
        #[serde(default)]
        pub user_id: String,
    }

    impl Default for ComposioPolicy {
        fn default() -> Self {
            Self { draft_stop: true, consent_acknowledged: false, user_id: String::new() }
        }
    }

    /// Load the Composio policy from `<app-data>/composio.json`. Returns `Default` on any
    /// read/parse failure — the safe-blocked state is always the fallback.
    pub(crate) fn load_composio_policy(app: &tauri::AppHandle) -> ComposioPolicy {
        use tauri::Manager;
        let path = match app.path().app_data_dir() {
            Ok(d) => d.join("composio.json"),
            Err(_) => return ComposioPolicy::default(),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return ComposioPolicy::default(),
        };
        serde_json::from_str::<ComposioPolicy>(&text).unwrap_or_default()
    }

    /// Decision oracle: is a live Composio send allowed given this policy and the current plan?
    ///
    /// Passes through the full type-safe gate from `shogun_mcp::composio`:
    ///   1. `consent_acknowledged` must be true — only then is `grant_consent` called.
    ///   2. `grant_consent(all-ack'd)` must succeed → produces a `ComposioConsent`.
    ///   3. `ComposioSender::new(consent)` starts with `draft_stop = true`.
    ///   4. `set_draft_stop(policy.draft_stop)` — the persisted setting is applied.
    ///   5. `send_capability(&ent)` returns `Some` only when draft-stop is OFF **and** the plan
    ///      holds the Composio send unlock (issue #97: Pro / active trial). The entitlement gate
    ///      composes with — never replaces — the consent and draft-stop gates.
    ///
    /// Pure: no I/O. Tested directly in the unit tests below.
    pub(crate) fn composio_send_allowed(
        policy: ComposioPolicy,
        ent: &shogun_agents::entitlement::Entitlements,
    ) -> bool {
        if !policy.consent_acknowledged {
            return false;
        }
        let disclosures = shogun_mcp::composio::Disclosures {
            via_third_party: true,
            data_types: true,
            revocable: true,
        };
        let consent = match shogun_mcp::composio::grant_consent(disclosures) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let mut sender = shogun_mcp::composio::ComposioSender::new(consent);
        sender.set_draft_stop(policy.draft_stop);
        sender.send_capability(ent).is_some()
    }

    /// The ONE shared L3 approval queue (B-3 / E-08): created once at startup and managed in
    /// Tauri state. Every producer (an agent, `submit_send`, a future in-app API/MCP face — which
    /// would receive a clone of this same `Arc`) enqueues here, and the settings `ApprovalsSection`
    /// is the single drain. No other `ApprovalQueue` may be constructed in this app.
    pub struct ApprovalQueueState(pub Arc<Mutex<ApprovalQueue>>);

    impl Default for ApprovalQueueState {
        fn default() -> Self {
            Self(Arc::new(Mutex::new(ApprovalQueue::new())))
        }
    }

    /// One row for the L3 confirm UI (FR-AG-03: op type, full destination, full body, route;
    /// B-3: plus which surface enqueued it — "ui" / "api" / "mcp").
    #[derive(serde::Serialize)]
    pub struct ApprovalView {
        pub id: u64,
        pub op_type: &'static str,
        pub destination: String,
        pub full_body: String,
        pub route: &'static str,
        pub origin: &'static str,
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

    /// L5 feedback scope for an approval item (Plan D-2 / D-6 scope rule). Deliberately simple
    /// and derived from the item itself: an email's recipient address is a stable person handle,
    /// so an email send is **person-scoped** with the address as `scope_ref` (resolving the
    /// address to a `people` row is left to distillation-time joins — the raw handle is
    /// deterministic and, like all feedback text, local-DB-only). Every other send kind has no
    /// resolvable person, so it is **global-scoped** and distinguished by `action_kind`.
    fn feedback_scope(action: &SendAction) -> (LessonScope, Option<&str>, &'static str) {
        match action {
            SendAction::SendEmail { to } => (LessonScope::Person, Some(to.as_str()), "send_email"),
            SendAction::PostMessage { .. } => (LessonScope::Global, None, "post_message"),
            SendAction::CreateCalendarEvent { .. } => {
                (LessonScope::Global, None, "create_calendar_event")
            }
            SendAction::PostComment { .. } => (LessonScope::Global, None, "post_comment"),
        }
    }

    /// Fire-and-forget L5 feedback write (Plan D-2). Failure is swallowed by `Db::record_feedback`
    /// itself (returns `None`); nothing here can block or fail the approval action, and the body
    /// text is never logged.
    fn record_approval_feedback(
        db: &shogun_core::daemon::Db,
        kind: FeedbackKind,
        action: &SendAction,
        before: Option<&str>,
        after: Option<&str>,
    ) {
        let (scope, scope_ref, action_kind) = feedback_scope(action);
        let _ = db.record_feedback(
            kind,
            scope,
            &NewFeedback {
                ts_ms: db.now_ms(),
                action_kind: Some(action_kind),
                scope_ref,
                before_text: before,
                after_text: after,
            },
        );
    }

    /// Enqueue a send for L3 confirmation from the webview. Returns the pending id.
    ///
    /// `ApprovalOrigin::Ui` is correct HERE because this is the Tauri command the panel calls;
    /// producers on other faces enqueue through their own entry points with their own origin.
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
        let id = {
            let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
            propose(&mut q, &proposal, ApprovalOrigin::Ui, now).0
        };
        // Something is waiting on a human decision — the first reason cues exist at all (#49).
        crate::sound::mac::play(shogun_core::sound::Cue::ApprovalPending);
        Ok(id)
    }

    /// What is being drafted. Four consecutive `&str` parameters is a transposition waiting to
    /// happen — `destination` and `subject` swapped would address a send at its own subject line —
    /// so they travel named.
    pub(crate) struct Draft<'a> {
        /// Send type: `email` / `slack` / `calendar` / `github`.
        pub kind: &'a str,
        /// Recipient / channel / title / target.
        pub destination: &'a str,
        pub subject: &'a str,
        /// The thread or screen text the draft is grounded in.
        pub context: &'a str,
    }

    /// The Reply Drafter flow itself (B-5), shared by the `draft_reply` command and the notch
    /// `DraftReply` dispatch (`notch_exec`): draft the body on the Agent lane (invariant 5) from
    /// `context`, then enqueue it on the ONE shared approval queue with the given origin. Blocking
    /// (an LLM call) — callers off the UI thread only. Returns the pending approval id; the human
    /// still confirms before anything sends (FR-AG-03).
    ///
    /// `origin` is a real parameter rather than a hardcoded `Ui`: lib.rs anticipates an in-app
    /// API/MCP face, and that face inheriting a "the user did this" label is exactly the
    /// mislabelling the badge exists to prevent (shogun-mcp already tags Api/Mcp correctly).
    pub(crate) fn draft_and_enqueue(
        req: Draft<'_>,
        queue: &Arc<Mutex<ApprovalQueue>>,
        db: &Db,
        directives: &str,
        origin: ApprovalOrigin,
    ) -> Result<u64, String> {
        use shogun_core::llm::AgentClient;
        let Draft { kind, destination, subject, context } = req;
        let base_prompt = format!(
            "You are drafting a concise, professional {kind} reply. Use the context below; write \
             only the reply body, no preamble.\n\n--- context ---\n{context}"
        );
        let prompt = if directives.trim().is_empty() {
            base_prompt
        } else {
            format!("{}\n{}", directives.trim(), base_prompt)
        };
        // Draft through the same BYOK Agent-lane client as inline drafts (invariant 5). Traceability
        // is recorded by the client at the egress point.
        let agent = crate::inline_source::mac::build_agent(db)
            .ok_or_else(|| "No key yet — add your provider key in Settings to draft replies.".to_string())?;
        let body = agent.complete(&prompt).map_err(|e| format!("draft failed: {e:?}"))?;

        let proposal = proposed(kind, destination, subject, &body)?;
        let now = db.now_ms().max(0) as u64;
        let id = {
            let mut q = queue.lock().map_err(|_| "approval queue poisoned".to_string())?;
            propose(&mut q, &proposal, origin, now).0
        };
        // A draft the user did not watch being written is exactly the case that needs telling (#49).
        crate::sound::mac::play(shogun_core::sound::Cue::ApprovalPending);
        Ok(id)
    }

    /// Reply Drafter (FR-AG-10) and the other draft-then-send agents: draft the body on the BYOK
    /// Agent lane (invariant 5) from the given context, then enqueue it as an L3 proposal. `kind`
    /// selects the send type (email/slack/calendar/github); `destination` is the recipient/channel/
    /// title/target; `context` is the thread/screen text the draft is grounded in. Returns the
    /// pending approval id. The human still confirms before anything sends (FR-AG-03).
    #[tauri::command]
    pub async fn draft_reply(
        kind: String,
        destination: String,
        subject: String,
        context: String,
        state: tauri::State<'_, ApprovalQueueState>,
        db: tauri::State<'_, Db>,
        user_cfg: tauri::State<'_, crate::user_config_watch::UserConfigState>,
    ) -> Result<u64, String> {
        // draft_and_enqueue blocks on an LLM round-trip (its contract says "callers off the UI
        // thread only") — a sync command would freeze the whole AppKit main thread for it.
        let queue = state.0.clone();
        let db = db.inner().clone();
        let directives = user_cfg.directives();
        tauri::async_runtime::spawn_blocking(move || {
            draft_and_enqueue(
                Draft {
                    kind: &kind,
                    destination: &destination,
                    subject: &subject,
                    context: &context,
                },
                &queue,
                &db,
                &directives,
                ApprovalOrigin::Ui,
            )
        })
        .await
        .map_err(|e| format!("draft task failed: {e}"))?
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
            .filter_map(|id| q.preview(id).map(|p| (id, p, q.origin(id))))
            .map(|(id, p, origin)| ApprovalView {
                id: id.0,
                op_type: p.op_type,
                destination: p.destination.clone(),
                full_body: p.full_body.clone(),
                route: route_str(p.route),
                // Every pending entry has an origin; default defensively rather than dropping the row.
                // "unknown", never "ui": the badge exists to tell the human WHICH face asked to
                // send something, and defaulting an unattributable request to "you did this" is
                // the wrong direction for a security disclosure. It should read stranger, not
                // safer.
                origin: origin.map_or("unknown", ApprovalOrigin::as_str),
            })
            .collect();
        Ok(views)
    }

    /// Reject a pending send. Records the L5 `reject` signal (Plan D-2) — fire-and-forget, after
    /// the queue decision, so a feedback failure can never affect the rejection itself.
    #[tauri::command]
    pub fn reject_send(
        id: u64,
        state: tauri::State<'_, ApprovalQueueState>,
        db: tauri::State<'_, Db>,
    ) -> Result<String, String> {
        let mut q = state.0.lock().map_err(|_| "approval queue poisoned".to_string())?;
        use shogun_agents::approval::RejectCause;
        // Snapshot action + proposed body before the reject dequeues them (feedback input only).
        let snapshot = q
            .action(ApprovalId(id))
            .cloned()
            .and_then(|a| q.preview(ApprovalId(id)).map(|p| (a, p.full_body.clone())));
        match q.reject(ApprovalId(id), RejectCause::UserRejected) {
            Decision::Rejected(_) => {
                if let Some((action, proposed_body)) = snapshot {
                    record_approval_feedback(
                        &db,
                        FeedbackKind::Reject,
                        &action,
                        Some(&proposed_body),
                        None,
                    );
                }
                Ok("rejected".into())
            }
            other => Ok(format!("{other:?}")),
        }
    }

    /// Confirm a pending send via the dedicated button (FR-AG-03) and execute it. Enter-key intent
    /// must be sent as a separate flag by the UI; this command is the button path.
    ///
    /// Plan gate (issue #97): executing any L3 send requires `agent_execution` (Pro / active
    /// trial) — checked first, core-side, before the queue is touched. Returns `plan_required`
    /// and leaves the item pending when the plan does not cover it.
    ///
    /// For Composio (email) sends the gate is consulted BEFORE the send is attempted:
    ///   - If the persisted `ComposioPolicy` has `consent_acknowledged = false` OR `draft_stop =
    ///     true`, the send is blocked and a Gmail draft is saved instead (FR-C2-02 / FR-C2-03).
    ///   - Only when both gates are open (consent ✓, draft-stop OFF) is the Composio key required
    ///     and the live send attempted.
    ///   - First-layer sends (Slack/calendar/GitHub) are completely unaffected by this gate.
    /// `edited_body` (optional) is the user's final text when the confirm UI offered an edit
    /// field (B-5): when present and different from the proposal it replaces the sent body — the
    /// human approved the *edited* text — and is recorded as the L5 `edit_before_approve` signal
    /// (Plan D-2). Callers that pass nothing get the unchanged flow (`approve_unchanged`).
    #[tauri::command]
    pub fn confirm_send(
        id: u64,
        edited_body: Option<String>,
        state: tauri::State<'_, ApprovalQueueState>,
        connectors: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
        app: tauri::AppHandle,
    ) -> Result<String, String> {
        let now = db.now_ms().max(0) as u64;
        // Plan gate FIRST (issue #97), before the confirm dequeues anything: executing any L3 send
        // is agent execution — Pro / active trial only. Checked core-side on every confirm so a
        // trial expiring while a send sits in the queue still blocks it. The item stays pending
        // (it can be confirmed after an upgrade, until the 10-minute window expires it).
        let ent = crate::entitlement::mac::current(&app);
        if !ent.agent_execution {
            return Ok("plan_required".into());
        }
        // Keep the runtime's WP-F double gate honest too: it re-checks authorize_op (which now
        // includes the plan) before any first-layer write executes.
        if let Ok(mut rt) = connectors.0.lock() {
            rt.set_plan(ent);
        }
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

        // --- L5 feedback hook (Plan D-2), fire-and-forget ---------------------------------------
        // An approval with an edited body is the richest correction signal we get; an unchanged
        // approval is the countersignal. Recorded at the decision point (the user approved),
        // independent of whether the transport later succeeds. A feedback failure is swallowed
        // inside `Db::record_feedback` and can never block the send below.
        let confirmed = match edited_body.filter(|b| *b != confirmed.preview.full_body) {
            Some(final_body) => {
                record_approval_feedback(
                    &db,
                    FeedbackKind::EditBeforeApprove,
                    &confirmed.action,
                    Some(&confirmed.preview.full_body),
                    Some(&final_body),
                );
                // The human approved the edited text, so that is what must send. Rebuild the
                // preview from the same action + route so the trace, the preview, and the wire
                // can never disagree (invariant 3).
                let preview = Preview::for_send(&confirmed.action, final_body, confirmed.preview.route);
                ConfirmedSend { action: confirmed.action, preview }
            }
            None => {
                record_approval_feedback(
                    &db,
                    FeedbackKind::ApproveUnchanged,
                    &confirmed.action,
                    None,
                    None,
                );
                confirmed
            }
        };

        // --- Composio consent + draft-stop gate (FR-C2-02 / FR-C2-03) -------------------------
        // Only Composio (email) sends are gated — first-layer sends bypass this entirely.
        use shogun_integrations::send_bridge::{route_send, SendRoute};
        if matches!(route_send(&confirmed.action), SendRoute::Composio) {
            let policy = load_composio_policy(&app);
            if !composio_send_allowed(policy, &ent) {
                // Gate is closed: save a draft instead of sending. Body/recipient are NOT logged
                // (invariant 7). The draft_fallback is the authoritative path for this so we reuse
                // it directly.
                let sink = db.traceability_sink();
                match save_gmail_draft(&connectors.0, &sink, &confirmed.action, &confirmed.preview.full_body) {
                    Ok(()) => {
                        return Ok("draft_saved: composio send is off (opt-in required)".into());
                    }
                    Err(e) => {
                        return Ok(format!("draft_save_failed: composio send is off (opt-in required); draft error: {e}"));
                    }
                }
            }
            // Gate is open — require the Composio API key before proceeding.
            let composio_key = composio_api_key()
                .filter(|k| !k.trim().is_empty())
                .ok_or_else(|| "Composio key not set — add it in settings to send".to_string())?;
            let composio_user = {
                let p = load_composio_policy(&app);
                if !p.user_id.trim().is_empty() {
                    p.user_id
                } else {
                    std::env::var("SHOGUN_COMPOSIO_USER_ID").unwrap_or_default()
                }
            };
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
            return match execute_send(&confirmed, &routed, &db.traceability_sink()) {
                SendExecOutcome::Sent => Ok("sent".into()),
                SendExecOutcome::Failed(e) => Ok(format!("failed:{e}")),
            };
        }

        // --- First-layer send (Slack / calendar / GitHub): no Composio gate, no Composio key ----
        // We confirmed above (the `SendRoute::Composio` branch returned early) that this action is
        // NOT an email send, so `FirstLayerSendTransport` alone is the right executor — no Composio
        // client or key is needed or consulted.
        let first_layer = FirstLayerSendTransport::new(&connectors.0);
        match execute_send(&confirmed, &first_layer, &db.traceability_sink()) {
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
        // Key names are the arg contract of `create_draft` in `ComposioReadRpc`, which maps
        // `to`/`subject`/`body` to Composio field names (`recipient_email`/`subject`/`body`).
        let args = json!({ "to": to, "subject": subject, "body": mail_body });
        let rt = runtime.lock().map_err(|_| "runtime lock poisoned".to_string())?;
        rt.execute_write_owned(shogun_mcp::scope::Service::Gmail, "draft_create_update", args).map(|_| ())?;
        // Record traceability only on success: the draft body just left the device via Composio.
        // Route::Composio = second-layer (third-party relay); third_party = true. Chunk is digested
        // and dropped — body text never reaches storage (G8 / invariant 3).
        sink.record(TraceRecord::for_chunk(
            Route::Composio,
            "draft_create",
            "gmail",
            &mail_body,
            true,
        ));
        Ok(())
    }

    /// The Composio API key is a plain secret (not a TokenSet) — read it directly from the Keychain.
    pub(crate) fn composio_api_key() -> Option<String> {
        keychain_store::get_generic_secret(COMPOSIO_KEY_ACCOUNT)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    }

    // ---- Composio settings commands (UI ↔ Rust) -----------------------------------------------

    /// Save the Composio API key to the Keychain (invariant 7 — never a file/env/DB/log).
    /// Trims whitespace; rejects empty keys. The key value is NEVER logged.
    ///
    /// After a successful save the connector runtime is rebuilt so the new key is picked up
    /// immediately without restarting the app. A missing or poisoned `ConnectorState` is not fatal
    /// — the save still succeeds; the runtime will use the new key on next restart.
    #[tauri::command]
    pub fn set_composio_key(
        key: String,
        connectors: tauri::State<'_, crate::connectors::mac::ConnectorState>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("key is empty".into());
        }
        // A real Composio API key is long; reject an obviously-wrong short paste (also keeps the
        // last-4 read-back from ever being close to the whole value).
        if key.chars().count() < 8 {
            return Err("key looks too short — check you pasted the full Composio API key".into());
        }
        keychain_store::set_generic_secret(COMPOSIO_KEY_ACCOUNT, key.as_bytes())
        .map_err(|e| e.to_string())?;
        eprintln!("[composio] api key saved to Keychain");
        // Rebuild the live runtime so the new key is used immediately.
        let policy = load_composio_policy(&app);
        if let Err(e) =
            crate::connectors::mac::rebuild_gmail_runtime(&connectors, &app, policy.draft_stop)
        {
            eprintln!("[connectors] runtime rebuild after key save skipped: {e}");
        }
        Ok(())
    }

    /// Remove the Composio API key from the Keychain. Not-found is silently ignored.
    #[tauri::command]
    pub fn clear_composio_key() -> Result<(), String> {
        match keychain_store::delete_generic_secret(COMPOSIO_KEY_ACCOUNT) {
            Ok(()) => {}
            Err(e) if e.code() == -25300 /* errSecItemNotFound */ => {}
            Err(e) => return Err(e.to_string()),
        }
        eprintln!("[composio] api key removed");
        Ok(())
    }

    /// Validation helper (pure; testable without I/O).
    /// Returns `true` when the policy combination is internally consistent.
    /// The only invalid combination is draft_stop=false AND consent_acknowledged=false:
    /// live sending may only be enabled once the user has acknowledged the disclosures.
    fn policy_is_valid(draft_stop: bool, consent: bool) -> bool {
        consent || draft_stop
    }

    /// Return a new policy with `user_id` set to `id`, preserving all other fields.
    fn with_user_id(p: ComposioPolicy, id: &str) -> ComposioPolicy {
        ComposioPolicy { user_id: id.to_string(), ..p }
    }

    /// Return a new policy with `draft_stop` and `consent_acknowledged` updated, preserving `user_id`.
    fn with_flags(p: ComposioPolicy, draft_stop: bool, consent: bool) -> ComposioPolicy {
        ComposioPolicy { draft_stop, consent_acknowledged: consent, ..p }
    }

    /// Persist the Composio opt-in policy to `<app-data>/composio.json`.
    fn save_composio_policy(app: &tauri::AppHandle, policy: ComposioPolicy) -> Result<(), String> {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("composio.json");
        let json = serde_json::to_string_pretty(&policy).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("save failed: {e}"))
    }

    /// Update the persisted Composio policy. Enforces the invariant that draft_stop may only be
    /// turned OFF once consent has been acknowledged (FR-C2-02 / FR-C2-03). Preserves `user_id`.
    #[tauri::command]
    pub fn set_composio_policy(
        draft_stop: bool,
        consent_acknowledged: bool,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        if !policy_is_valid(draft_stop, consent_acknowledged) {
            return Err("consent required before enabling sending".into());
        }
        let existing = load_composio_policy(&app);
        save_composio_policy(&app, with_flags(existing, draft_stop, consent_acknowledged))
    }

    /// Persist the Composio user ID (non-secret) to the policy JSON file. Preserves all other
    /// policy fields. Trims whitespace; an empty string clears the stored ID (env fallback applies).
    ///
    /// After a successful save the connector runtime is rebuilt so the new user_id is picked up
    /// immediately. A missing or poisoned `ConnectorState` is not fatal — the save still succeeds.
    #[tauri::command]
    pub fn set_composio_user_id(
        user_id: String,
        connectors: tauri::State<'_, crate::connectors::mac::ConnectorState>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let trimmed = user_id.trim().to_string();
        let policy = load_composio_policy(&app);
        let draft_stop = policy.draft_stop;
        save_composio_policy(&app, with_user_id(policy, &trimmed))?;
        // Rebuild the live runtime so the new user_id is used immediately.
        if let Err(e) =
            crate::connectors::mac::rebuild_gmail_runtime(&connectors, &app, draft_stop)
        {
            eprintln!("[connectors] runtime rebuild after user_id save skipped: {e}");
        }
        Ok(())
    }

    /// The view model returned to the UI for the Composio settings block.
    #[derive(serde::Serialize)]
    pub struct ComposioSettingsView {
        /// Whether a Composio API key is stored in the Keychain.
        pub has_key: bool,
        /// Last 4 characters of the stored key, or empty string if no key.
        pub key_last4: String,
        /// Current draft-stop setting (true = draft only; false = live send allowed).
        pub draft_stop: bool,
        /// Whether the user has completed the consent disclosure flow.
        pub consent_acknowledged: bool,
        /// The stored Composio user ID (non-secret). Empty string if not yet configured.
        pub user_id: String,
    }

    /// Return the current Composio settings for the UI (key presence, last4, policy flags, user ID).
    /// Secrets are NEVER returned in full — only the last 4 characters (invariant 7).
    #[tauri::command]
    pub fn composio_settings(app: tauri::AppHandle) -> ComposioSettingsView {
        let policy = load_composio_policy(&app);
        let user_id = policy.user_id.clone();
        let (has_key, key_last4) = match composio_api_key() {
            Some(k) if !k.trim().is_empty() => {
                let k = k.trim();
                // Last 4 chars only, char-safe. A key too short to have 4 chars is masked
                // entirely rather than echoed — never return the whole secret (invariant 7).
                let n = k.chars().count();
                let last4 = if n >= 4 { k.chars().skip(n - 4).collect() } else { "····".to_string() };
                (true, last4)
            }
            _ => (false, String::new()),
        };
        ComposioSettingsView { has_key, key_last4, draft_stop: policy.draft_stop, consent_acknowledged: policy.consent_acknowledged, user_id }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A plan holding the Composio send unlock (Pro), so these tests exercise the
        /// consent/draft-stop gates in isolation.
        fn pro() -> shogun_agents::entitlement::Entitlements {
            shogun_agents::entitlement::entitlements(shogun_agents::entitlement::Plan::Pro, 0)
        }

        /// Issue #97: a plan without the send unlock blocks the send even with consent given and
        /// draft-stop OFF — the entitlement gate composes with the older gates.
        #[test]
        fn plan_without_unlock_blocks_send_despite_open_policy() {
            use shogun_agents::entitlement::{entitlements, Plan, TRIAL_DURATION_MS};
            let policy = ComposioPolicy { consent_acknowledged: true, draft_stop: false, user_id: String::new() };
            let standard = entitlements(Plan::Standard, 0);
            let expired = entitlements(Plan::Trial { started_at_ms: Some(0) }, TRIAL_DURATION_MS);
            assert!(!composio_send_allowed(policy.clone(), &standard));
            assert!(!composio_send_allowed(policy, &expired));
        }

        /// Default policy: draft_stop = true, consent_acknowledged = false → blocked.
        #[test]
        fn default_policy_blocks_send() {
            let policy = ComposioPolicy::default();
            assert!(policy.draft_stop, "draft_stop must default ON");
            assert!(!policy.consent_acknowledged, "consent must default NOT acknowledged");
            assert!(!composio_send_allowed(policy, &pro()), "default policy must block the send");
        }

        /// consent = true, draft_stop = true → still blocked (draft-stop gate).
        #[test]
        fn consent_true_draftstop_true_blocks_send() {
            let policy = ComposioPolicy { consent_acknowledged: true, draft_stop: true, user_id: String::new() };
            assert!(!composio_send_allowed(policy, &pro()), "draft-stop ON must block even with consent");
        }

        /// consent = false, draft_stop = false → blocked (consent gate).
        #[test]
        fn consent_false_draftstop_false_blocks_send() {
            let policy = ComposioPolicy { consent_acknowledged: false, draft_stop: false, user_id: String::new() };
            assert!(!composio_send_allowed(policy, &pro()), "no consent must block even when draft-stop is OFF");
        }

        /// consent = true, draft_stop = false → allowed (both gates open).
        #[test]
        fn consent_true_draftstop_false_allows_send() {
            let policy = ComposioPolicy { consent_acknowledged: true, draft_stop: false, user_id: String::new() };
            assert!(composio_send_allowed(policy, &pro()), "consent + draft-stop OFF must allow the send");
        }

        // ---- policy_is_valid: all 4 combinations ---------------------------------------------

        /// consent=false, draft_stop=false → invalid (only invalid combination).
        #[test]
        fn composio_policy_invalid_no_consent_no_draftstop() {
            assert!(!policy_is_valid(false, false), "draft_stop=false + consent=false must be invalid");
        }

        /// consent=false, draft_stop=true → valid (draft-stop engaged protects even without consent).
        #[test]
        fn composio_policy_valid_draftstop_on_no_consent() {
            assert!(policy_is_valid(true, false), "draft_stop=true + consent=false must be valid");
        }

        /// consent=true, draft_stop=true → valid (consent given, draft-stop still on).
        #[test]
        fn composio_policy_valid_consent_draftstop_on() {
            assert!(policy_is_valid(true, true), "draft_stop=true + consent=true must be valid");
        }

        /// consent=true, draft_stop=false → valid (consent given, live send enabled).
        #[test]
        fn composio_policy_valid_consent_draftstop_off() {
            assert!(policy_is_valid(false, true), "draft_stop=false + consent=true must be valid");
        }

        // ---- user_id field: serde defaults and helper functions -----------------------------

        #[test]
        fn serde_default_user_id() {
            let json = r#"{"draft_stop":true,"consent_acknowledged":false}"#;
            let p: ComposioPolicy = serde_json::from_str(json).expect("deserialize");
            assert_eq!(p.user_id, "", "user_id should default to empty string");
        }

        #[test]
        fn round_trip_all_fields() {
            let original = ComposioPolicy { draft_stop: false, consent_acknowledged: true, user_id: "test-user-123".to_string() };
            let json = serde_json::to_string(&original).expect("serialize");
            let loaded: ComposioPolicy = serde_json::from_str(&json).expect("deserialize");
            assert!(!loaded.draft_stop);
            assert!(loaded.consent_acknowledged);
            assert_eq!(loaded.user_id, "test-user-123");
        }

        #[test]
        fn with_user_id_preserves_flags() {
            let p = ComposioPolicy { draft_stop: false, consent_acknowledged: true, user_id: String::new() };
            let updated = with_user_id(p, "new-user");
            assert_eq!(updated.user_id, "new-user");
            assert!(!updated.draft_stop);
            assert!(updated.consent_acknowledged);
        }

        #[test]
        fn with_flags_preserves_user_id() {
            let p = ComposioPolicy { draft_stop: true, consent_acknowledged: false, user_id: "preserved-user".to_string() };
            let updated = with_flags(p, false, true);
            assert_eq!(updated.user_id, "preserved-user");
            assert!(!updated.draft_stop);
            assert!(updated.consent_acknowledged);
        }
    }
}
