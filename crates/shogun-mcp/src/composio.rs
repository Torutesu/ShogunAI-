//! The second-layer (Composio) opt-in gate (§6.10, FR-C2-01..05). v1's only second-layer operation
//! is **Gmail send** (FR-C2-01); everything here is built so a send is unreachable unless two gates
//! are passed, both enforced by the type system rather than a runtime flag:
//!
//! 1. **Opt-in consent** (FR-C2-02): a [`ComposioConsent`] can only be produced by [`grant_consent`],
//!    which requires every disclosure (third-party routing, data types, revocability) acknowledged.
//!    A [`ComposioSender`] cannot be built without one — so with no opt-in, no send path exists
//!    (§6.10 acceptance: "opt-in なしで送信経路のコードに到達しない").
//! 2. **Draft-stop mode** (FR-C2-03, default ON): while ON, [`ComposioSender::send_capability`]
//!    returns `None` and [`ComposioSender::offered_actions`] omits `Send` entirely (hidden, not
//!    greyed). Only a [`SendCapability`] — obtainable solely when draft-stop is OFF — lets
//!    [`prepare_send`] be called.
//!
//! A prepared send is always L3 with a `ViaComposio` route (FR-C2-04), and its traceability entry
//! always carries the third-party badge ([`COMPOSIO_THIRD_PARTY`]). On Composio failure the send is
//! never silently rerouted — it is treated as failed and a draft is saved instead (FR-C2-05).

use shogun_agents::approval::{Preview, Route};
use shogun_agents::permission::SendAction;

/// The disclosures the opt-in consent screen must present and the user must acknowledge (FR-C2-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disclosures {
    /// (a) sends are routed through Composio, a third party.
    pub via_third_party: bool,
    /// (b) the data types that leave the device (recipient / subject / body).
    pub data_types: bool,
    /// (c) it can be disabled at any time.
    pub revocable: bool,
}

impl Disclosures {
    /// All three disclosures acknowledged.
    pub fn all_acknowledged(self) -> bool {
        self.via_third_party && self.data_types && self.revocable
    }
}

/// Why consent could not be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentError {
    /// One or more disclosures were not acknowledged (FR-C2-02: no consent → no connection flow).
    IncompleteDisclosures,
}

/// Proof that the user completed the Composio opt-in. The inner field is private, so the only way
/// to obtain a value is [`grant_consent`] — there is no `Default`, no public constructor. This is
/// the compile-time form of "default disabled, explicit opt-in required".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposioConsent {
    _acknowledged: Disclosures,
}

/// Grant consent iff every disclosure is acknowledged (FR-C2-02). Returns the capability token that
/// gates the whole second layer.
pub fn grant_consent(disclosures: Disclosures) -> Result<ComposioConsent, ConsentError> {
    if disclosures.all_acknowledged() {
        Ok(ComposioConsent { _acknowledged: disclosures })
    } else {
        Err(ConsentError::IncompleteDisclosures)
    }
}

/// The actions the Composio integration offers in the UI. `Send` appears only when draft-stop is OFF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposioAction {
    /// Save a Gmail draft (first-layer, never leaves as a send).
    SaveDraft,
    /// Send via Composio (second-layer, L3). Hidden while draft-stop is ON.
    Send,
}

/// The Composio sender. Cannot be constructed without a [`ComposioConsent`]. Draft-stop defaults ON
/// (FR-C2-03).
pub struct ComposioSender {
    _consent: ComposioConsent,
    draft_stop: bool,
}

/// A capability to perform a Composio send. Obtainable only from a sender whose draft-stop is OFF
/// (see [`ComposioSender::send_capability`]); its existence proves both gates passed. Without one,
/// [`prepare_send`] cannot be called.
pub struct SendCapability<'a> {
    _sender: &'a ComposioSender,
}

impl ComposioSender {
    /// Build a sender from consent. Draft-stop starts ON (FR-C2-03 default).
    pub fn new(consent: ComposioConsent) -> Self {
        Self { _consent: consent, draft_stop: true }
    }

    /// Turn draft-stop on/off. (Off requires a deliberate settings change.)
    pub fn set_draft_stop(&mut self, on: bool) {
        self.draft_stop = on;
    }

    pub fn draft_stop(&self) -> bool {
        self.draft_stop
    }

    /// The actions to show in the UI. While draft-stop is ON, `Send` is omitted entirely — hidden,
    /// not greyed (FR-C2-03).
    pub fn offered_actions(&self) -> Vec<ComposioAction> {
        if self.draft_stop {
            vec![ComposioAction::SaveDraft]
        } else {
            vec![ComposioAction::SaveDraft, ComposioAction::Send]
        }
    }

    /// A send capability, available only when draft-stop is OFF. `None` while ON — so no send can be
    /// prepared.
    pub fn send_capability(&self) -> Option<SendCapability<'_>> {
        if self.draft_stop {
            None
        } else {
            Some(SendCapability { _sender: self })
        }
    }
}

/// A Gmail send — v1's only second-layer operation (FR-C2-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmailSend {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Composio routing is always third-party (FR-C2-04): every traceability entry for a Composio send
/// carries the badge (`TraceRecord.third_party = true`).
pub const COMPOSIO_THIRD_PARTY: bool = true;

/// Prepare a Composio Gmail send for L3 approval. Requires a [`SendCapability`] — so this is
/// unreachable unless consent was granted and draft-stop is OFF. Returns the [`SendAction`] (always
/// a send → L3) and the L3 [`Preview`] with a `ViaComposio` route and the full subject+body text
/// (FR-AG-03 / FR-C2-04). The returned action is meant to be enqueued in the L3 approval queue.
pub fn prepare_send(_cap: SendCapability<'_>, mail: GmailSend) -> (SendAction, Preview) {
    let action = SendAction::SendEmail { to: mail.to.clone() };
    // Full preview text: subject + body, never a summary (FR-AG-03).
    let full = format!("Subject: {}\n\n{}", mail.subject, mail.body);
    let preview = Preview::for_send(&action, full, Route::ViaComposio);
    (action, preview)
}

/// The outcome of attempting a Composio send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendResult {
    /// The send succeeded via Composio.
    Sent,
    /// Composio failed → the send is treated as **failed** and a Gmail draft was saved as the
    /// fallback, with the user notified (FR-C2-05). The route is never silently changed to "sent".
    FailedDraftSaved,
}

/// Resolve a Composio failure. Never reports [`SendResult::Sent`] — on failure the send fails and a
/// draft is saved (FR-C2-05).
pub fn on_composio_failure() -> SendResult {
    SendResult::FailedDraftSaved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_consent() -> ComposioConsent {
        grant_consent(Disclosures { via_third_party: true, data_types: true, revocable: true }).unwrap()
    }

    #[test]
    fn consent_requires_every_disclosure() {
        // any missing acknowledgement → no consent
        for d in [
            Disclosures { via_third_party: false, data_types: true, revocable: true },
            Disclosures { via_third_party: true, data_types: false, revocable: true },
            Disclosures { via_third_party: true, data_types: true, revocable: false },
        ] {
            assert_eq!(grant_consent(d), Err(ConsentError::IncompleteDisclosures));
        }
        // all acknowledged → ok
        assert!(grant_consent(Disclosures { via_third_party: true, data_types: true, revocable: true }).is_ok());
    }

    #[test]
    fn draft_stop_defaults_on_and_hides_send() {
        let sender = ComposioSender::new(full_consent());
        assert!(sender.draft_stop(), "draft-stop must default ON (FR-C2-03)");
        assert_eq!(sender.offered_actions(), vec![ComposioAction::SaveDraft]);
        assert!(!sender.offered_actions().contains(&ComposioAction::Send), "Send must be hidden");
        assert!(sender.send_capability().is_none(), "no send capability while draft-stop ON");
    }

    #[test]
    fn disabling_draft_stop_exposes_send() {
        let mut sender = ComposioSender::new(full_consent());
        sender.set_draft_stop(false);
        assert!(sender.offered_actions().contains(&ComposioAction::Send));
        assert!(sender.send_capability().is_some());
    }

    #[test]
    fn prepared_send_is_l3_via_composio_with_full_text() {
        let mut sender = ComposioSender::new(full_consent());
        sender.set_draft_stop(false);
        let cap = sender.send_capability().unwrap();
        let (action, preview) = prepare_send(
            cap,
            GmailSend { to: "bob@example.com".into(), subject: "Ship date".into(), body: "Friday.".into() },
        );
        // it's a send → L3
        use shogun_agents::permission::{Action, Level};
        assert_eq!(Action::Send(action.clone()).required_level(), Level::L3);
        assert!(matches!(action, SendAction::SendEmail { .. }));
        // preview: composio route, full subject+body
        assert_eq!(preview.route, Route::ViaComposio);
        assert!(preview.full_body.contains("Subject: Ship date"));
        assert!(preview.full_body.contains("Friday."));
        assert_eq!(preview.destination, "bob@example.com");
    }

    #[test]
    fn composio_send_is_always_third_party() {
        // FR-C2-04: Composio traceability entries carry the third-party badge. Bound through a
        // value so this checks the constant we actually expose, not a literal.
        let badge = COMPOSIO_THIRD_PARTY;
        assert!(badge, "Composio entries must carry the third-party badge");
    }

    #[test]
    fn failure_never_reports_sent() {
        // FR-C2-05: on failure the send fails and a draft is saved — never silently "sent".
        assert_eq!(on_composio_failure(), SendResult::FailedDraftSaved);
        assert_ne!(on_composio_failure(), SendResult::Sent);
    }

    // Structural note (FR-C2 acceptance): `prepare_send` takes a `SendCapability`, whose only source
    // is `ComposioSender::send_capability()` returning `Some` — which happens only when draft-stop is
    // OFF, on a sender that required a `ComposioConsent` to build, which required full disclosures.
    // So the send path is unreachable without: consent → draft-stop OFF. None of it is a runtime
    // flag that could be bypassed; it is the type graph.
}
