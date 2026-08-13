//! Cross-crate invariant-4 guard: **an L1 (auto-executed) path can never reach an external send.**
//!
//! Each crate tests its own slice of this (permission levels, preset tables, MCP scope, Composio).
//! This integration test asserts the guarantee holds *across the whole surface at once*, so a future
//! change that quietly lets a send through at L1 in any one layer fails here even if that crate's
//! own tests were adjusted. shogun-mcp is the natural host: it can see the agents permission model,
//! the preset tables, its own scope table, and the Composio gate together.
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
        SendAction::CreateCalendarEvent {
            title: "t".into(), start_time: "2026-08-13T10:00:00Z".into(),
            end_time: "2026-08-13T11:00:00Z".into(), calendar_id: None, description: "body".into(),
        },
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

    // default draft-stop ON → no send capability exists.
    assert!(sender.draft_stop());
    assert!(sender.send_capability().is_none());

    // turning it off yields a capability; the prepared action is an L3 send.
    sender.set_draft_stop(false);
    let cap = sender.send_capability().expect("capability once draft-stop is off");
    let (action, preview) = shogun_mcp::composio::prepare_send(
        cap,
        GmailSend { to: "z@y.com".into(), subject: "s".into(), body: "b".into() },
    );
    assert!(matches!(action, SendAction::SendEmail { .. }));
    assert_eq!(Action::Send(action).required_level(), Level::L3);
    assert_eq!(preview.route, shogun_agents::approval::Route::ViaComposio);
}
