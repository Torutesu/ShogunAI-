//! Post-approval L3 send execution with mandatory traceability (WP4.3, §6.14 / invariant 3).
//!
//! This is the write-path counterpart to [`crate::daemon::Db::ingest_integration`] (the read path):
//! once the [`ApprovalQueue`](shogun_agents::approval::ApprovalQueue) hands back a
//! [`ConfirmedSend`], the send actually leaves the device here — and **exactly when it does, a
//! traceability record is written**. The recording is structural, not a matter of discipline:
//!
//! - The record is built from the [`ConfirmedSend`]'s own preview, so the traced route/destination
//!   and the digested body can never disagree with what was sent.
//! - The record is written **only on a successful egress** (transport `Ok`). If the send fails
//!   (not connected, token expired), nothing left the device, so nothing is traced — symmetric with
//!   the read path, which traces nothing because a read carries no user data off-device.
//! - [`TraceRecord`] has no text field, so the sent body is digested and dropped; the body never
//!   reaches storage (G8).
//!
//! The transport is a seam ([`SendTransport`]): the real remote-MCP / Composio client needs OAuth
//! tokens (Category C) and lands later, but the whole record-on-send guarantee is exercised here on
//! Linux with a fake transport.

use shogun_agents::approval::{ConfirmedSend, Route as ApprovalRoute};
use shogun_agents::permission::SendAction;

use crate::llm::traceability::{Route, TraceRecord, TraceabilitySink};

/// The seam that actually performs an external send. The real implementation is a remote-MCP /
/// Composio client (Category C — needs connected OAuth tokens); tests inject a fake. It is handed
/// the send action and the full body; it must return a non-sensitive error string on failure (no
/// body text in the error).
pub trait SendTransport {
    fn send(&self, action: &SendAction, body: &str) -> Result<(), String>;
}

/// The outcome of executing a confirmed send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendExecOutcome {
    /// The send left the device and a traceability record was written.
    Sent,
    /// The transport failed; nothing left the device and nothing was traced. Carries the
    /// non-sensitive error string.
    Failed(String),
}

/// Map the approval-layer route onto the traceability route. A direct send goes over the service's
/// official MCP; a Composio-relayed send is the third-party route (surfaced as such in the viewer).
fn trace_route(route: ApprovalRoute) -> Route {
    match route {
        ApprovalRoute::DirectMcp => Route::Mcp,
        ApprovalRoute::ViaComposio => Route::Composio,
    }
}

/// A stable machine purpose string for a send, stored in the traceability log (FR-TR-01).
fn purpose_for(action: &SendAction) -> &'static str {
    match action {
        SendAction::SendEmail { .. } => "integration.send_email",
        SendAction::PostMessage { .. } => "integration.post_message",
        SendAction::AddReaction { .. } => "integration.add_reaction",
        SendAction::CreateCalendarEvent { .. } => "integration.create_calendar_event",
        SendAction::UpdateCalendarEvent { .. } => "integration.update_calendar_event",
        SendAction::PostComment { .. } => "integration.post_comment",
        SendAction::CreateDocument { .. } => "integration.create_document",
        SendAction::UpdateDocument { .. } => "integration.update_document",
        SendAction::ChangeIssueStatus { .. } => "integration.change_issue_status",
    }
}

/// Build the traceability record for a confirmed send — pure, digests the body and drops it. The
/// route, destination, and third-party flag all come from the confirmed preview so the trace
/// describes exactly what will be sent.
pub fn trace_for_send(confirmed: &ConfirmedSend) -> TraceRecord {
    let preview = &confirmed.preview;
    let route = trace_route(preview.route);
    TraceRecord::for_chunk(
        route,
        purpose_for(&confirmed.action),
        preview.destination.clone(),
        &preview.full_body,
        preview.route == ApprovalRoute::ViaComposio,
    )
}

/// Execute a confirmed L3 send: record traceability to `sink`, then attempt the transport. This is
/// the single point through which a first-layer send reaches the wire, so every send that leaves
/// the device leaves a trace (invariant 3 / FR-TR-03).
///
/// The trace is written **before** the transport call — the same rule every LLM client in this
/// crate applies ("record before the request: that is the true egress point"). A send that fails
/// at the HTTP layer has already delivered the body to the third party (Composio answering 500
/// still ingested the email), so tracing only on `Ok` under-reports real egress. The cost is the
/// opposite corner: a transport that refuses locally (wrong route, poisoned lock) leaves a trace
/// for bytes that never left. Over-reporting an attempt is the safe direction for a disclosure
/// log; silently missing a third-party crossing is the failure mode invariant 3 exists to prevent.
pub fn execute_send<T: SendTransport + ?Sized, S: TraceabilitySink + ?Sized>(
    confirmed: &ConfirmedSend,
    transport: &T,
    sink: &S,
) -> SendExecOutcome {
    sink.record(trace_for_send(confirmed));
    match transport.send(&confirmed.action, &confirmed.preview.full_body) {
        Ok(()) => SendExecOutcome::Sent,
        Err(e) => SendExecOutcome::Failed(e),
    }
}

/// The first-layer [`SendTransport`]: routes a confirmed send through the connector runtime
/// (WP-F). Routing is `shogun_integrations::send_bridge` (which service + scope op performs this
/// action); execution is [`ConnectorRuntime::execute_write`], which re-applies the service gate —
/// so even a confirmed send is refused if the service is unreleased / disconnected (double gate).
/// An email send is refused here outright: it is the second layer's (Composio, §6.10), never MCP.
pub struct FirstLayerSendTransport<'a, T> {
    runtime: &'a std::sync::Mutex<shogun_integrations::ConnectorRuntime<T>>,
}

impl<'a, T> FirstLayerSendTransport<'a, T> {
    pub fn new(runtime: &'a std::sync::Mutex<shogun_integrations::ConnectorRuntime<T>>) -> Self {
        Self { runtime }
    }
}

impl<T> SendTransport for FirstLayerSendTransport<'_, T>
where
    T: shogun_mcp::sync::IntegrationTransport + shogun_integrations::WriteExecutor,
{
    fn send(&self, action: &SendAction, body: &str) -> Result<(), String> {
        use shogun_integrations::send_bridge::{args_for_send, route_send, SendRoute};
        match route_send(action) {
            SendRoute::Composio => {
                Err("email send is second-layer (Composio, opt-in) — not first-layer MCP".to_string())
            }
            SendRoute::FirstLayer { service, op } => {
                let args = args_for_send(action, body);
                let rt = self.runtime.lock().map_err(|_| "runtime lock poisoned".to_string())?;
                rt.execute_write_owned(service, op, args).map(|_| ())
            }
        }
    }
}

/// A [`SendTransport`] that performs a Gmail send through Composio (second layer, §6.10 / FR-C2-01).
/// Pure over the [`ComposioApi`](shogun_integrations::composio::ComposioApi) seam — the reqwest
/// client is [`crate::composio_send::HttpComposioApi`] (feature `net`). Handles **only** the email
/// send (v1's single second-layer op); any other action is refused (it is the first layer's).
pub struct ComposioSendTransport<A: shogun_integrations::composio::ComposioApi> {
    api: A,
    /// This device's Composio user identifier for the connected Gmail account.
    user_id: String,
}

impl<A: shogun_integrations::composio::ComposioApi> ComposioSendTransport<A> {
    pub fn new(api: A, user_id: impl Into<String>) -> Self {
        Self { api, user_id: user_id.into() }
    }
}

impl<A: shogun_integrations::composio::ComposioApi> SendTransport for ComposioSendTransport<A> {
    fn send(&self, action: &SendAction, body: &str) -> Result<(), String> {
        use shogun_integrations::composio::{gmail_send_arguments, parse_execute_response, GMAIL_SEND_EMAIL};
        let SendAction::SendEmail { to } = action else {
            return Err("ComposioSendTransport only performs Gmail send (second layer)".to_string());
        };
        // `body` is the confirmed full preview ("Subject: …\n\n…"); split it back for Composio.
        let (subject, mail_body) = shogun_mcp::composio::parse_gmail_full_body(body);
        let args = gmail_send_arguments(to, &subject, &mail_body);
        let resp = self.api.execute(GMAIL_SEND_EMAIL, &self.user_id, args)?;
        parse_execute_response(&resp)
    }
}

/// A [`SendTransport`] that dispatches by route ([`shogun_integrations::send_bridge::route_send`]):
/// the email send goes through Composio (second layer, §6.10); everything else goes through the
/// first layer. On a Composio failure it applies the FR-C2-05 fallback — save a Gmail draft via the
/// injected `draft_fallback` — and still reports failure (the send did not happen; nothing is
/// traced by [`execute_send`]). This is the one transport the daemon hands to `execute_send`.
/// Saves a device-visible Gmail draft when a Composio send fails (FR-C2-05). Wired by the daemon to
/// a first-layer Gmail `draft_create_update`.
pub type DraftFallback = Box<dyn Fn(&SendAction, &str) -> Result<(), String> + Send + Sync>;

pub struct RoutedSendTransport<C: SendTransport, F: SendTransport> {
    composio: C,
    first_layer: F,
    draft_fallback: DraftFallback,
}

impl<C: SendTransport, F: SendTransport> RoutedSendTransport<C, F> {
    pub fn new(composio: C, first_layer: F, draft_fallback: DraftFallback) -> Self {
        Self { composio, first_layer, draft_fallback }
    }
}

impl<C: SendTransport, F: SendTransport> SendTransport for RoutedSendTransport<C, F> {
    fn send(&self, action: &SendAction, body: &str) -> Result<(), String> {
        use shogun_integrations::send_bridge::{route_send, SendRoute};
        match route_send(action) {
            SendRoute::Composio => match self.composio.send(action, body) {
                Ok(()) => Ok(()),
                Err(e) => {
                    // FR-C2-05: the send failed → save a draft, but never report it as sent.
                    match (self.draft_fallback)(action, body) {
                        Ok(()) => Err(format!("composio send failed ({e}); Gmail draft saved")),
                        Err(de) => {
                            Err(format!("composio send failed ({e}); draft fallback also failed ({de})"))
                        }
                    }
                }
            },
            SendRoute::FirstLayer { .. } => self.first_layer.send(action, body),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::{digest, RecordingSink};
    use shogun_agents::approval::{Preview, Route as ApprovalRoute};

    fn email_send() -> SendAction {
        SendAction::SendEmail { to: "alice@example.com".into() }
    }

    fn confirmed(route: ApprovalRoute, body: &str) -> ConfirmedSend {
        let action = email_send();
        let preview = Preview::for_send(&action, body, route);
        ConfirmedSend { action, preview }
    }

    /// A transport that succeeds, or fails with a fixed error.
    struct Fake {
        ok: bool,
    }
    impl SendTransport for Fake {
        fn send(&self, _action: &SendAction, _body: &str) -> Result<(), String> {
            if self.ok {
                Ok(())
            } else {
                Err("not connected".into())
            }
        }
    }

    #[test]
    fn successful_send_records_exactly_one_trace_with_digest_only() {
        let sink = RecordingSink::new();
        let body = "Hi Alice — shipping the roadmap Friday.";
        let cs = confirmed(ApprovalRoute::DirectMcp, body);

        let outcome = execute_send(&cs, &Fake { ok: true }, &sink);
        assert_eq!(outcome, SendExecOutcome::Sent);

        let recs = sink.records();
        assert_eq!(recs.len(), 1, "a send that reached the wire writes exactly one trace");
        let rec = &recs[0];
        assert_eq!(rec.route, Route::Mcp);
        assert_eq!(rec.purpose, "integration.send_email");
        assert_eq!(rec.destination, "alice@example.com");
        assert_eq!(rec.chunk_bytes, body.len());
        assert_eq!(rec.chunk_xxh64, digest(body));
        assert!(!rec.third_party, "a direct MCP send is not third-party");
        // the body text never appears in the record
        assert!(!format!("{rec:?}").contains("roadmap"), "sent body must never appear in the trace");
    }

    #[test]
    fn composio_send_is_traced_third_party() {
        let sink = RecordingSink::new();
        let cs = confirmed(ApprovalRoute::ViaComposio, "body");
        assert_eq!(execute_send(&cs, &Fake { ok: true }, &sink), SendExecOutcome::Sent);
        let recs = sink.records();
        assert_eq!(recs[0].route, Route::Composio);
        assert!(recs[0].third_party, "a Composio-relayed send is disclosed third-party (FR-C2-04)");
    }

    #[test]
    fn failed_send_is_still_traced() {
        // The trace is written before the transport call: an HTTP-level failure (Composio 500)
        // has already delivered the body to the third party, so tracing only on Ok would hide
        // real egress. Over-reporting a local refusal is the accepted cost (see execute_send).
        let sink = RecordingSink::new();
        let cs = confirmed(ApprovalRoute::DirectMcp, "body that may have left");
        let outcome = execute_send(&cs, &Fake { ok: false }, &sink);
        assert_eq!(outcome, SendExecOutcome::Failed("not connected".into()));
        assert_eq!(sink.records().len(), 1, "the attempt itself is disclosed");
    }

    #[test]
    fn purpose_is_per_action() {
        let cal = ConfirmedSend {
            action: SendAction::CreateCalendarEvent { title: "Sync".into() },
            preview: Preview::for_send(
                &SendAction::CreateCalendarEvent { title: "Sync".into() },
                "Sync 3pm",
                ApprovalRoute::DirectMcp,
            ),
        };
        assert_eq!(trace_for_send(&cal).purpose, "integration.create_calendar_event");
    }

    // ---- FirstLayerSendTransport (WP-F) -------------------------------------------------------

    use shogun_integrations::{ConnectorRuntime, WriteExecutor};
    use shogun_mcp::scope::{Service, Wave};
    use shogun_mcp::sync::{FetchedItem, IntegrationTransport};
    use std::cell::RefCell;
    use std::sync::Mutex;

    /// The write the runtime's own transport recorded (shared so a test can observe writes that go
    /// through the runtime, not a separate executor).
    type Executed = std::rc::Rc<RefCell<Option<(Service, String, serde_json::Value)>>>;

    /// A fake that satisfies both seams: no-op reads, and records the executed write.
    struct FakeMcp {
        executed: Executed,
    }
    impl IntegrationTransport for FakeMcp {
        fn read_sync(&self, _s: Service) -> Result<Vec<FetchedItem>, String> {
            Ok(vec![])
        }
    }
    impl WriteExecutor for FakeMcp {
        fn execute(&self, s: Service, op: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
            *self.executed.borrow_mut() = Some((s, op.to_string(), args));
            Ok(serde_json::json!({ "ok": true }))
        }
    }

    /// Build a runtime whose own transport records writes, plus the handle to inspect them.
    fn runtime_at(wave: Wave, connect: &[Service]) -> (Mutex<ConnectorRuntime<FakeMcp>>, Executed) {
        let executed: Executed = std::rc::Rc::new(RefCell::new(None));
        let mut rt = ConnectorRuntime::new(FakeMcp { executed: executed.clone() }, wave, true);
        for &s in connect {
            rt.mark_connected(s, 1_000);
        }
        (Mutex::new(rt), executed)
    }

    #[test]
    fn confirmed_calendar_send_executes_the_mapped_op_and_traces() {
        let (rt, executed) = runtime_at(Wave::One, &[Service::GoogleCalendar]);
        let transport = FirstLayerSendTransport::new(&rt);

        let action = SendAction::CreateCalendarEvent { title: "Sync".into() };
        let preview = Preview::for_send(&action, "agenda body", ApprovalRoute::DirectMcp);
        let sink = RecordingSink::new();
        let out = execute_send(&ConfirmedSend { action, preview }, &transport, &sink);

        assert_eq!(out, SendExecOutcome::Sent);
        // The scope op (not the raw tool) was dispatched with the confirmed content.
        let (svc, op, args) = executed.borrow().clone().unwrap();
        assert_eq!((svc, op.as_str()), (Service::GoogleCalendar, "event_create"));
        assert_eq!(args["summary"], "Sync");
        assert_eq!(args["description"], "agenda body");
        // Exactly one trace, direct-MCP route.
        assert_eq!(sink.records().len(), 1);
    }

    #[test]
    fn email_send_is_refused_as_second_layer_and_never_reaches_the_first_layer() {
        let (rt, executed) = runtime_at(Wave::One, &[Service::Gmail]);
        let transport = FirstLayerSendTransport::new(&rt);

        let cs = confirmed(ApprovalRoute::DirectMcp, "body");
        let sink = RecordingSink::new();
        let out = execute_send(&cs, &transport, &sink);

        assert!(matches!(out, SendExecOutcome::Failed(ref e) if e.contains("Composio")));
        assert!(executed.borrow().is_none(), "no first-layer write may run for an email send");
        // The attempt is traced (execute_send records before the transport decides); the refusal
        // means the trace over-reports, which is the accepted direction (see execute_send).
        assert_eq!(sink.records().len(), 1);
    }

    #[test]
    fn unreleased_wave_refuses_even_a_confirmed_send_double_gate() {
        // Slack post confirmed, but Slack is Wave 2 and only Wave 1 is released — the runtime's
        // gate refuses it even post-approval (WP-F double gate).
        let (rt, executed) = runtime_at(Wave::One, &[Service::Slack]);
        let transport = FirstLayerSendTransport::new(&rt);

        let action = SendAction::PostMessage { channel: "#general".into() };
        let preview = Preview::for_send(&action, "hello", ApprovalRoute::DirectMcp);
        let sink = RecordingSink::new();
        let out = execute_send(&ConfirmedSend { action, preview }, &transport, &sink);

        assert!(matches!(out, SendExecOutcome::Failed(_)));
        assert!(executed.borrow().is_none());
        // Traced as an attempt even though the double gate refused it (record-before-send).
        assert_eq!(sink.records().len(), 1);
    }

    // ---- ComposioSendTransport + RoutedSendTransport (WP-D) -----------------------------------

    use shogun_integrations::composio::ComposioApi;
    use serde_json::{json, Value};

    struct FakeComposio {
        last: RefCell<Option<(String, String, Value)>>,
        reply: Result<Value, String>,
    }
    impl ComposioApi for FakeComposio {
        fn execute(&self, tool: &str, user_id: &str, args: Value) -> Result<Value, String> {
            *self.last.borrow_mut() = Some((tool.to_string(), user_id.to_string(), args));
            self.reply.clone()
        }
    }

    #[test]
    fn composio_email_send_executes_gmail_send_email_and_traces() {
        let api = FakeComposio { last: RefCell::new(None), reply: Ok(json!({ "successful": true })) };
        let transport = ComposioSendTransport::new(api, "user-42");
        // The Composio preview carries the ViaComposio route (third-party badge in the trace).
        let cs = confirmed(ApprovalRoute::ViaComposio, "Subject: Ship date\n\nFriday.");
        let sink = RecordingSink::new();

        let out = execute_send(&cs, &transport, &sink);
        assert_eq!(out, SendExecOutcome::Sent);
        let (tool, user, args) = transport.api.last.borrow().clone().unwrap();
        assert_eq!((tool.as_str(), user.as_str()), ("GMAIL_SEND_EMAIL", "user-42"));
        assert_eq!(args["recipient_email"], "alice@example.com");
        assert_eq!(args["subject"], "Ship date");
        assert_eq!(args["body"], "Friday.");
        // Exactly one trace, carrying the third-party (Composio) badge.
        let recs = sink.records();
        assert_eq!(recs.len(), 1);
        assert!(recs[0].third_party, "a Composio send must be traced third-party (FR-C2-04)");
    }

    #[test]
    fn composio_failure_saves_a_draft_and_still_reports_failure_frc205() {
        let api = FakeComposio { last: RefCell::new(None), reply: Err("composio http 500".into()) };
        let composio = ComposioSendTransport::new(api, "u");
        // First-layer arm is unused for an email; a no-op fake satisfies the type.
        let (rt, _executed) = runtime_at(Wave::One, &[]);
        let first_layer = FirstLayerSendTransport::new(&rt);

        use std::sync::atomic::{AtomicBool, Ordering};
        let draft_saved = std::sync::Arc::new(AtomicBool::new(false));
        let ds = draft_saved.clone();
        let routed = RoutedSendTransport::new(
            composio,
            first_layer,
            Box::new(move |_a, _b| {
                ds.store(true, Ordering::SeqCst);
                Ok(())
            }),
        );

        let cs = confirmed(ApprovalRoute::ViaComposio, "Subject: x\n\nbody");
        let sink = RecordingSink::new();
        let out = execute_send(&cs, &routed, &sink);

        // FR-C2-05: failed send, draft saved — and the attempt IS traced: the body reached
        // Composio before the 500 came back, so this is exactly the third-party crossing the
        // trace log must not miss (record-before-send, see execute_send).
        match out {
            SendExecOutcome::Failed(e) => assert!(e.contains("draft saved"), "got: {e}"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(draft_saved.load(Ordering::SeqCst), "a Gmail draft must be saved on Composio failure");
        let recs = sink.records();
        assert_eq!(recs.len(), 1, "the failed Composio send still crossed the third-party boundary");
        assert!(recs[0].third_party);
    }

    #[test]
    fn routed_sends_non_email_through_the_first_layer() {
        let api = FakeComposio { last: RefCell::new(None), reply: Ok(json!({ "successful": true })) };
        let composio = ComposioSendTransport::new(api, "u");
        let (rt, executed) = runtime_at(Wave::One, &[Service::GoogleCalendar]);
        let first_layer = FirstLayerSendTransport::new(&rt);
        let routed = RoutedSendTransport::new(composio, first_layer, Box::new(|_a, _b| Ok(())));

        let action = SendAction::CreateCalendarEvent { title: "Sync".into() };
        let preview = Preview::for_send(&action, "agenda", ApprovalRoute::DirectMcp);
        let sink = RecordingSink::new();
        let out = execute_send(&ConfirmedSend { action, preview }, &routed, &sink);

        assert_eq!(out, SendExecOutcome::Sent);
        // The first layer executed the calendar op (the Composio arm is only for email).
        let (svc, op, _) = executed.borrow().clone().unwrap();
        assert_eq!((svc, op.as_str()), (Service::GoogleCalendar, "event_create"));
    }
}
