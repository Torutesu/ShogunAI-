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

    /// Decision oracle: is a live Composio send allowed given this policy?
    ///
    /// Passes through the full type-safe gate from `shogun_mcp::composio`:
    ///   1. `consent_acknowledged` must be true — only then is `grant_consent` called.
    ///   2. `grant_consent(all-ack'd)` must succeed → produces a `ComposioConsent`.
    ///   3. `ComposioSender::new(consent)` starts with `draft_stop = true`.
    ///   4. `set_draft_stop(policy.draft_stop)` — the persisted setting is applied.
    ///   5. `send_capability()` returns `Some` only when draft-stop is OFF.
    ///
    /// Pure: no I/O. Tested directly in the unit tests below.
    pub(crate) fn composio_send_allowed(policy: ComposioPolicy) -> bool {
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
        sender.send_capability().is_some()
    }

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
    ///
    /// For Composio (email) sends the gate is consulted BEFORE the send is attempted:
    ///   - If the persisted `ComposioPolicy` has `consent_acknowledged = false` OR `draft_stop =
    ///     true`, the send is blocked and a Gmail draft is saved instead (FR-C2-02 / FR-C2-03).
    ///   - Only when both gates are open (consent ✓, draft-stop OFF) is the Composio key required
    ///     and the live send attempted.
    ///   - First-layer sends (Slack/calendar/GitHub) are completely unaffected by this gate.
    #[tauri::command]
    pub fn confirm_send(
        id: u64,
        state: tauri::State<'_, ApprovalQueueState>,
        connectors: tauri::State<'_, ConnectorState>,
        db: tauri::State<'_, Db>,
        app: tauri::AppHandle,
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

        // --- Composio consent + draft-stop gate (FR-C2-02 / FR-C2-03) -------------------------
        // Only Composio (email) sends are gated — first-layer sends bypass this entirely.
        use shogun_integrations::send_bridge::{route_send, SendRoute};
        if matches!(route_send(&confirmed.action), SendRoute::Composio) {
            let policy = load_composio_policy(&app);
            if !composio_send_allowed(policy) {
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
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, COMPOSIO_KEY_ACCOUNT)
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
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            COMPOSIO_KEY_ACCOUNT,
            key.as_bytes(),
        )
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
        match security_framework::passwords::delete_generic_password(
            KEYCHAIN_SERVICE,
            COMPOSIO_KEY_ACCOUNT,
        ) {
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

        /// Default policy: draft_stop = true, consent_acknowledged = false → blocked.
        #[test]
        fn default_policy_blocks_send() {
            let policy = ComposioPolicy::default();
            assert!(policy.draft_stop, "draft_stop must default ON");
            assert!(!policy.consent_acknowledged, "consent must default NOT acknowledged");
            assert!(!composio_send_allowed(policy), "default policy must block the send");
        }

        /// consent = true, draft_stop = true → still blocked (draft-stop gate).
        #[test]
        fn consent_true_draftstop_true_blocks_send() {
            let policy = ComposioPolicy { consent_acknowledged: true, draft_stop: true, user_id: String::new() };
            assert!(!composio_send_allowed(policy), "draft-stop ON must block even with consent");
        }

        /// consent = false, draft_stop = false → blocked (consent gate).
        #[test]
        fn consent_false_draftstop_false_blocks_send() {
            let policy = ComposioPolicy { consent_acknowledged: false, draft_stop: false, user_id: String::new() };
            assert!(!composio_send_allowed(policy), "no consent must block even when draft-stop is OFF");
        }

        /// consent = true, draft_stop = false → allowed (both gates open).
        #[test]
        fn consent_true_draftstop_false_allows_send() {
            let policy = ComposioPolicy { consent_acknowledged: true, draft_stop: false, user_id: String::new() };
            assert!(composio_send_allowed(policy), "consent + draft-stop OFF must allow the send");
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
            assert_eq!(loaded.draft_stop, false);
            assert_eq!(loaded.consent_acknowledged, true);
            assert_eq!(loaded.user_id, "test-user-123");
        }

        #[test]
        fn with_user_id_preserves_flags() {
            let p = ComposioPolicy { draft_stop: false, consent_acknowledged: true, user_id: String::new() };
            let updated = with_user_id(p, "new-user");
            assert_eq!(updated.user_id, "new-user");
            assert_eq!(updated.draft_stop, false);
            assert_eq!(updated.consent_acknowledged, true);
        }

        #[test]
        fn with_flags_preserves_user_id() {
            let p = ComposioPolicy { draft_stop: true, consent_acknowledged: false, user_id: "preserved-user".to_string() };
            let updated = with_flags(p, false, true);
            assert_eq!(updated.user_id, "preserved-user");
            assert_eq!(updated.draft_stop, false);
            assert_eq!(updated.consent_acknowledged, true);
        }
    }
}
