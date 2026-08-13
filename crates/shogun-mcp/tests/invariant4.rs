//! Cross-crate invariant-4 guard: **an L1 (auto-executed) path can never reach an external send.**
//!
//! Each crate tests its own slice of this (permission levels, preset tables, MCP scope, Composio).
//! This integration test asserts the guarantee holds *across the whole surface at once*, so a future
//! change that quietly lets a send through at L1 in any one layer fails here even if that crate's
//! own tests were adjusted. shogun-mcp is the natural host: it can see the agents permission model,
//! the preset tables, its own scope table, and the Composio gate together.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use shogun_agents::entitlement::{entitlements, Plan, TRIAL_DURATION_MS};
use shogun_agents::permission::{Action, Level, LocalAction, SendAction};
use shogun_agents::presets::{OpKind, PRESETS};
use shogun_mcp::composio::{grant_consent, ComposioSender, Disclosures, GmailSend};
use shogun_mcp::scope::{authorize, Authorization, Service, ALL_SERVICES};

/// permission model: every send is L3, no local action is a send, no local action is L3.
#[test]
fn permission_model_keeps_sends_at_l3() {
    let sends = [
        SendAction::SendEmail { to: "a@b.com".into() },
        SendAction::PostMessage { channel: "#x".into() },
        SendAction::CreateCalendarEvent { title: "t".into() },
        SendAction::PostComment { target: "r#1".into() },
    ];
    for s in sends {
        let a = Action::Send(s);
        assert_eq!(a.required_level(), Level::L3);
        assert!(!a.is_l1_eligible());
        assert!(a.is_external_send());
    }

    let locals = [
        LocalAction::OpenApp { bundle_id: "x".into() },
        LocalAction::LocalSearch { query: "q".into() },
        LocalAction::SaveDraft { target: "reply" },
        LocalAction::CopyToClipboard { text: "c".into() },
        LocalAction::UpdateState { table: "people", state_id: 1 },
    ];
    for l in locals {
        let a = Action::Local(l);
        assert_ne!(a.required_level(), Level::L3, "no local action is L3");
        assert!(!a.is_external_send(), "a local action is never a send");
    }
}

/// preset tables: every ExternalSend op is L3, and no L1 op is a send.
#[test]
fn preset_sends_are_all_l3() {
    for p in PRESETS {
        for op in p.operations {
            if op.kind == OpKind::ExternalSend {
                assert_eq!(op.level, Level::L3, "{}::{} send must be L3", p.name, op.name);
            }
            if op.level == Level::L1 {
                assert!(!op.kind.is_external_send(), "{}::{} L1 must not be a send", p.name, op.name);
            }
        }
    }
}

/// MCP scope tables: no external-send operation is ever authorized below L3 (L3, Composio, or
/// not-implemented only) — never Background, never L1/L2.
#[test]
fn mcp_scope_never_authorizes_a_send_below_l3() {
    for &service in ALL_SERVICES {
        for op in shogun_mcp::scope::scope(service).ops {
            if op.class.is_external_send() {
                match authorize(service, op.name) {
                    Authorization::RequiresLevel(Level::L3)
                    | Authorization::RequiresComposio
                    | Authorization::DeniedNotImplemented => {}
                    other => panic!("{service:?}::{} send authorized as {other:?} (must be L3)", op.name),
                }
            }
        }
    }
    // and Gmail send specifically is first-layer-denied → Composio.
    assert_eq!(authorize(Service::Gmail, "send"), Authorization::RequiresComposio);
}

/// Composio gate: a prepared send is always an L3 send, and the send path is unreachable while
/// draft-stop is ON (the default) — no capability, so prepare_send cannot be called.
#[test]
fn composio_send_is_l3_and_gated_by_draft_stop() {
    let consent =
        grant_consent(Disclosures { via_third_party: true, data_types: true, revocable: true }).unwrap();
    let mut sender = ComposioSender::new(consent);

    let pro = entitlements(Plan::Pro, 0);
    // default draft-stop ON → no send capability exists, even for Pro.
    assert!(sender.draft_stop());
    assert!(sender.send_capability(&pro).is_none());

    // turning it off yields a capability (on an entitled plan); the prepared action is an L3 send.
    sender.set_draft_stop(false);
    let cap = sender.send_capability(&pro).expect("capability once draft-stop is off");
    let (action, preview) = shogun_mcp::composio::prepare_send(
        cap,
        GmailSend { to: "z@y.com".into(), subject: "s".into(), body: "b".into() },
    );
    assert!(matches!(action, SendAction::SendEmail { .. }));
    assert_eq!(Action::Send(action).required_level(), Level::L3);
    assert_eq!(preview.route, shogun_agents::approval::Route::ViaComposio);
}

/// B-3 / E-08 regression: **an L3 send submitted through the MCP or REST face lands in the ONE
/// shared approval queue** — the same queue the confirm UI drains — labeled with its origin, and
/// is resolvable (confirm / reject) from that queue regardless of origin. Before B-3 each face
/// constructed a private queue no UI ever drained; the constructor-injection this exercises is
/// what closes E-08.
#[test]
fn api_and_mcp_sends_land_in_the_one_shared_queue_with_origins() {
    use std::sync::{Arc, Mutex};

    use shogun_agents::approval::{
        ApprovalId, ApprovalOrigin, ApprovalQueue, ConfirmIntent, Decision, RejectCause,
    };
    use shogun_mcp::backend::StubBackend;
    use shogun_mcp::mcp::McpServer;
    use shogun_mcp::rest;

    // The one process-wide queue (what the desktop manages at startup / a bin creates at its
    // composition root). Both faces below receive THIS queue, not their own.
    let shared: Arc<Mutex<ApprovalQueue>> = Arc::new(Mutex::new(ApprovalQueue::new()));

    // 1. MCP face: actions.execute with a send → 202-equivalent pending, enqueued in `shared`.
    // Both faces are told a confirm UI exists — that is the premise of this test (the desktop
    // hosting them). A headless process refuses the send instead; see the test below.
    let server = McpServer::new(
        StubBackend,
        shared.clone(),
        || 1_000,
        shogun_agents::entitlement::Entitlements::trial_not_started,
    )
    .with_approval_surface(rest::ApprovalSurface::Present);
    let resp = server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"actions.execute","arguments":{"kind":"send_email","to":"a@b.com","subject":"s","body":"mcp draft"}}}"#,
        )
        .expect("a response line");
    // The tool result is a JSON string inside the JSON-RPC envelope, so the inner quotes arrive
    // escaped on the wire.
    assert!(resp.contains(r#"\"pending\":true"#), "MCP send must be pending, never executed: {resp}");
    assert!(resp.contains(r#"\"origin\":\"mcp\""#), "the MCP face labels its origin: {resp}");

    // 2. REST face: the same shared queue, Api origin.
    let (status, body) = {
        let mut q = shared.lock().expect("queue");
        rest::act(
            Some(r#"{"kind":"send_email","to":"b@c.com","subject":"s","body":"api draft"}"#),
            1_000,
            &mut q,
            ApprovalOrigin::Api,
            rest::ApprovalSurface::Present,
        )
    };
    assert_eq!(status, 202);
    assert!(body.contains("\"origin\":\"api\""), "{body}");

    // 3. One listing (what the UI's list_approvals drains) shows BOTH, each with its origin.
    let (mcp_id, api_id) = {
        let q = shared.lock().expect("queue");
        let ids = q.pending_ids();
        assert_eq!(ids.len(), 2, "both faces enqueued into the same queue");
        assert_eq!(q.origin(ids[0]), Some(ApprovalOrigin::Mcp));
        assert_eq!(q.origin(ids[1]), Some(ApprovalOrigin::Api));
        // full-content previews are present for the confirm UI (FR-AG-03)
        assert_eq!(q.preview(ids[0]).expect("preview").full_body, "Subject: s\n\nmcp draft");
        (ids[0], ids[1])
    };

    // 4. The UI-side confirm/reject operates on them identically, regardless of origin.
    {
        let mut q = shared.lock().expect("queue");
        assert!(matches!(
            q.confirm(mcp_id, ConfirmIntent::DedicatedButton, 2_000),
            Decision::Confirmed(cs) if cs.preview.full_body == "Subject: s\n\nmcp draft"
        ));
        assert_eq!(
            q.reject(api_id, RejectCause::UserRejected),
            Decision::Rejected(RejectCause::UserRejected)
        );
        assert_eq!(q.pending_len(), 0, "both resolved through the one queue");
    }

    // 5. L1-can-never-send stays intact on the same surface: a local action through the same
    //    faces executes locally and never touches the queue.
    let (status, body) = {
        let mut q = shared.lock().expect("queue");
        rest::act(Some(r#"{"kind":"local_search","query":"x"}"#), 1_000, &mut q, ApprovalOrigin::Api, rest::ApprovalSurface::Present)
    };
    assert_eq!(status, 200);
    assert!(body.contains("\"executed\":\"local\""));
    assert_eq!(shared.lock().expect("queue").pending_len(), 0, "a local action never enqueues");
    // and an unknown id is refused, not silently confirmed
    assert_eq!(
        shared.lock().expect("queue").confirm(ApprovalId(999), ConfirmIntent::DedicatedButton, 0),
        Decision::Unknown
    );
}

/// Issue #97 regression: **no send path bypasses BOTH the entitlement gate and the L3/consent
/// gates.** Every surface that could emit an external send is checked with a non-entitled plan
/// (Standard, expired trial) — each one must refuse before any send exists — and then with an
/// entitled plan, where the L3/consent machinery must still be the only way through.
#[test]
fn no_send_path_bypasses_entitlement_or_l3_gates() {
    let locked_plans = [
        entitlements(Plan::Standard, 0),
        entitlements(Plan::Trial { started_at_ms: Some(0) }, TRIAL_DURATION_MS),
    ];
    let pro = entitlements(Plan::Pro, 0);

    for locked in &locked_plans {
        // 1. Execution engine: a send (or anything else) submitted on a locked plan is rejected —
        //    it never reaches the effector, let alone the network.
        use shogun_agents::engine::{Disposition, ExecutionEngine, RejectReason};
        struct NoEffect;
        impl shogun_agents::engine::LocalEffector for NoEffect {
            fn run(&self, _a: &Action) -> Result<(), String> {
                panic!("effector must never run on a locked plan");
            }
        }
        struct NoObs;
        impl shogun_agents::engine::ExecutionObserver for NoObs {
            fn on_executed(&self, _i: shogun_agents::engine::ActionId, _a: &Action) {}
            fn on_rejected(&self, _i: shogun_agents::engine::ActionId, _a: &Action, _r: &RejectReason) {}
            fn on_cancelled(&self, _i: shogun_agents::engine::ActionId, _a: &Action) {}
            fn on_expired(&self, _i: shogun_agents::engine::ActionId, _a: &Action) {}
            fn on_failed(&self, _i: shogun_agents::engine::ActionId, _a: &Action, _e: &str) {}
        }
        let mut engine = ExecutionEngine::new(NoEffect, NoObs, 5000);
        let sub = engine.submit(Action::Send(SendAction::SendEmail { to: "a@b.com".into() }), 0, locked);
        assert_eq!(sub.disposition, Disposition::Rejected(RejectReason::PlanNotEntitled));

        // 2. Memory API dispatch: a send on a locked plan is denied BEFORE the approval queue.
        use shogun_agents::approval::{ApprovalQueue, Preview, Route};
        use shogun_mcp::dispatch::{ActionOutcome, Denied, MemoryApi};
        use shogun_mcp::memory_api::TokenRegistry;
        let mut tokens = TokenRegistry::new();
        tokens.issue("t");
        let mut approvals = ApprovalQueue::new();
        let send = SendAction::SendEmail { to: "a@b.com".into() };
        let preview = Preview::for_send(&send, "body", Route::ViaComposio);
        {
            let mut api = MemoryApi::new(&tokens, &mut approvals, *locked);
            assert_eq!(
                api.submit_send(Some("t"), send, preview, 0),
                ActionOutcome::Denied(Denied::PlanNotEntitled)
            );
        }
        assert_eq!(approvals.pending_len(), 0, "nothing may be enqueued on a locked plan");

        // 3. Service gate: every external-send op of every service is plan-denied.
        use shogun_mcp::connection::ConnState;
        use shogun_mcp::service_gate::{authorize_op, DenyReason, OpContext, OpDecision};
        use shogun_mcp::scope::Wave;
        let ctx = OpContext {
            highest_released: Wave::Three,
            conn: ConnState::Connected { last_sync_ms: 0 },
            draft_stop: false,
            plan: *locked,
        };
        for &service in ALL_SERVICES {
            for op in shogun_mcp::scope::scope(service).ops {
                if op.class.is_external_send() {
                    match authorize_op(service, op.name, &ctx) {
                        OpDecision::Denied(DenyReason::PlanNotEntitled)
                        | OpDecision::Denied(DenyReason::NotImplemented) => {}
                        other => panic!(
                            "{service:?}::{} allowed as {other:?} on a locked plan",
                            op.name
                        ),
                    }
                }
            }
        }

        // 4. Composio: even with full consent AND draft-stop OFF, a locked plan yields no
        //    capability — prepare_send is unreachable.
        let consent = grant_consent(Disclosures { via_third_party: true, data_types: true, revocable: true })
            .unwrap();
        let mut sender = ComposioSender::new(consent);
        sender.set_draft_stop(false);
        assert!(sender.send_capability(locked).is_none());
    }

    // And the converse: an entitled plan does NOT dissolve the older gates — consent and
    // draft-stop still decide, and the prepared send is still L3.
    let consent = grant_consent(Disclosures { via_third_party: true, data_types: true, revocable: true })
        .unwrap();
    let mut sender = ComposioSender::new(consent);
    assert!(sender.send_capability(&pro).is_none(), "draft-stop ON still blocks Pro");
    sender.set_draft_stop(false);
    let cap = sender.send_capability(&pro).expect("Pro + consent + draft-stop OFF");
    let (action, _preview) = shogun_mcp::composio::prepare_send(
        cap,
        GmailSend { to: "z@y.com".into(), subject: "s".into(), body: "b".into() },
    );
    assert_eq!(Action::Send(action).required_level(), Level::L3, "entitled send is still L3");
}
