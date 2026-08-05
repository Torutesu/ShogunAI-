//! The L1/L2 execution engine (WP3.3, §6.6.2) — the gate between a proposed [`Action`] and its
//! effect.
//!
//! The engine routes by permission level ([`Action::required_level`]):
//! - **L1** runs immediately (auto-execute) and reports `action.executed`.
//! - **L2** is queued awaiting a one-tap confirm; [`ExecutionEngine::confirm`] runs it (unless it
//!   has expired), [`ExecutionEngine::cancel`] drops it.
//! - **L3** (external sends) is **rejected here** — v1 has no L3 execution path; it opens in M4.
//!   Because a send is rejected rather than "queued as auto", invariant 4 holds at the engine
//!   boundary too: no code path in this engine can auto-run a send.
//!
//! Pure and platform-independent: OS effects go through [`LocalEffector`] and reporting
//! (event-log write + bus publish) through [`ExecutionObserver`], both injected by the daemon.
//! `now_ms` is a parameter, never a clock read, so queueing/expiry is deterministic under test.

use crate::permission::{Action, Level};

/// A handle for a submitted action (for confirm / cancel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionId(pub u64);

/// Why an action was refused at submit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    /// An external send (L3). v1 has no L3 execution path (opens in M4).
    ExternalSendNotAvailable,
}

/// What happened when an action was submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// L1: executed immediately.
    AutoRan,
    /// L1: the effector ran and reported an error. Distinct from [`Disposition::AutoRan`] so a
    /// caller keying off the return value (a UI writing "done") cannot mistake a failure for a
    /// success; the observer's `on_failed` carries the error detail.
    Failed,
    /// L2: queued, awaiting a one-tap confirm.
    AwaitingConfirm,
    /// Refused (see [`RejectReason`]).
    Rejected(RejectReason),
}

/// The result of submitting an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submitted {
    pub id: ActionId,
    pub disposition: Disposition,
}

/// The terminal outcome of a queued (L2) action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Ran successfully.
    Executed,
    /// The user cancelled it before confirming.
    Cancelled,
    /// The confirm window elapsed.
    Expired,
    /// The effector failed; carries the error string (never capture content).
    Failed(String),
    /// No pending action with that id.
    Unknown,
}

/// The OS-effect seam. The desktop app runs the real effect (open app, reveal file, save draft,
/// update state, …); tests inject a double. Only [`crate::permission::LocalAction`]s reach here —
/// sends never do — so an effector cannot be asked to send off-device.
pub trait LocalEffector {
    /// Run a local action. Returns an error string on failure (must not contain captured text).
    fn run(&self, action: &Action) -> Result<(), String>;
}

/// The reporting seam: the daemon wires this to the `action.executed` bus event + the event-log
/// write (FR-MEM-10). The engine calls it on every terminal transition.
pub trait ExecutionObserver {
    fn on_executed(&self, id: ActionId, action: &Action);
    fn on_rejected(&self, id: ActionId, action: &Action, reason: &RejectReason);
    fn on_cancelled(&self, id: ActionId, action: &Action);
    fn on_expired(&self, id: ActionId, action: &Action);
    fn on_failed(&self, id: ActionId, action: &Action, error: &str);
}

/// A queued L2 action awaiting confirmation.
#[derive(Debug, Clone)]
struct Pending {
    id: ActionId,
    action: Action,
    submitted_at_ms: u64,
}

/// The execution engine. Owns the pending-confirm queue and the confirm timeout.
pub struct ExecutionEngine<E: LocalEffector, O: ExecutionObserver> {
    effector: E,
    observer: O,
    next_id: u64,
    pending: Vec<Pending>,
    confirm_timeout_ms: u64,
}

impl<E: LocalEffector, O: ExecutionObserver> ExecutionEngine<E, O> {
    /// Build an engine with a confirm timeout (how long an L2 action waits for a tap).
    pub fn new(effector: E, observer: O, confirm_timeout_ms: u64) -> Self {
        Self { effector, observer, next_id: 1, pending: Vec::new(), confirm_timeout_ms }
    }

    fn alloc_id(&mut self) -> ActionId {
        let id = ActionId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Submit an action. L1 runs now; L2 is queued; L3 (send) is rejected.
    pub fn submit(&mut self, action: Action, now_ms: u64) -> Submitted {
        let id = self.alloc_id();
        match action.required_level() {
            Level::L1 => {
                let disposition = self.run_now(id, &action);
                Submitted { id, disposition }
            }
            Level::L2 => {
                self.pending.push(Pending { id, action, submitted_at_ms: now_ms });
                Submitted { id, disposition: Disposition::AwaitingConfirm }
            }
            Level::L3 => {
                let reason = RejectReason::ExternalSendNotAvailable;
                self.observer.on_rejected(id, &action, &reason);
                Submitted { id, disposition: Disposition::Rejected(reason) }
            }
        }
    }

    /// Run an action through the effector now, reporting the outcome. Returns [`Disposition::AutoRan`]
    /// on success — used for the L1 path (and internally by confirm).
    fn run_now(&self, id: ActionId, action: &Action) -> Disposition {
        match self.effector.run(action) {
            Ok(()) => {
                self.observer.on_executed(id, action);
                Disposition::AutoRan
            }
            Err(e) => {
                self.observer.on_failed(id, action, &e);
                Disposition::Failed
            }
        }
    }

    fn take_pending(&mut self, id: ActionId) -> Option<Pending> {
        let idx = self.pending.iter().position(|p| p.id == id)?;
        Some(self.pending.remove(idx))
    }

    /// Confirm a queued L2 action. Expired ones do not run.
    pub fn confirm(&mut self, id: ActionId, now_ms: u64) -> Outcome {
        let Some(p) = self.take_pending(id) else {
            return Outcome::Unknown;
        };
        if now_ms.saturating_sub(p.submitted_at_ms) > self.confirm_timeout_ms {
            self.observer.on_expired(id, &p.action);
            return Outcome::Expired;
        }
        match self.effector.run(&p.action) {
            Ok(()) => {
                self.observer.on_executed(id, &p.action);
                Outcome::Executed
            }
            Err(e) => {
                self.observer.on_failed(id, &p.action, &e);
                Outcome::Failed(e)
            }
        }
    }

    /// Cancel a queued L2 action before it is confirmed.
    pub fn cancel(&mut self, id: ActionId) -> Outcome {
        match self.take_pending(id) {
            Some(p) => {
                self.observer.on_cancelled(id, &p.action);
                Outcome::Cancelled
            }
            None => Outcome::Unknown,
        }
    }

    /// Expire every queued action past the confirm timeout. Returns the expired ids. The daemon
    /// calls this on a timer tick.
    pub fn expire_due(&mut self, now_ms: u64) -> Vec<ActionId> {
        let timeout = self.confirm_timeout_ms;
        let (expired, live): (Vec<Pending>, Vec<Pending>) = self
            .pending
            .drain(..)
            .partition(|p| now_ms.saturating_sub(p.submitted_at_ms) > timeout);
        self.pending = live;
        for p in &expired {
            self.observer.on_expired(p.id, &p.action);
        }
        expired.iter().map(|p| p.id).collect()
    }

    /// How many actions are queued awaiting confirmation.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{LocalAction, SendAction};
    use std::cell::RefCell;

    /// Records every observer callback and controls whether the effector succeeds.
    #[derive(Default)]
    struct Spy {
        events: RefCell<Vec<String>>,
        fail_with: Option<String>,
    }

    impl Spy {
        fn log(&self, s: String) {
            self.events.borrow_mut().push(s);
        }
        fn events(&self) -> Vec<String> {
            self.events.borrow().clone()
        }
    }

    // The engine takes effector + observer by value; use a shared &Spy via references implementing
    // both traits.
    impl LocalEffector for &Spy {
        fn run(&self, action: &Action) -> Result<(), String> {
            if let Some(e) = &self.fail_with {
                return Err(e.clone());
            }
            self.log(format!("run:{action:?}"));
            Ok(())
        }
    }
    impl ExecutionObserver for &Spy {
        fn on_executed(&self, id: ActionId, _a: &Action) {
            self.log(format!("executed:{}", id.0));
        }
        fn on_rejected(&self, id: ActionId, _a: &Action, r: &RejectReason) {
            self.log(format!("rejected:{}:{r:?}", id.0));
        }
        fn on_cancelled(&self, id: ActionId, _a: &Action) {
            self.log(format!("cancelled:{}", id.0));
        }
        fn on_expired(&self, id: ActionId, _a: &Action) {
            self.log(format!("expired:{}", id.0));
        }
        fn on_failed(&self, id: ActionId, _a: &Action, e: &str) {
            self.log(format!("failed:{}:{e}", id.0));
        }
    }

    fn l1() -> Action {
        Action::Local(LocalAction::LocalSearch { query: "budget".into() })
    }
    fn l2() -> Action {
        Action::Local(LocalAction::UpdateState { table: "people", state_id: 1 })
    }
    fn l3() -> Action {
        Action::Send(SendAction::SendEmail { to: "a@b.com".into() })
    }

    #[test]
    fn l1_auto_runs_and_reports() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let r = engine.submit(l1(), 0);
        assert_eq!(r.disposition, Disposition::AutoRan);
        assert_eq!(engine.pending_len(), 0);
        assert!(spy.events().iter().any(|e| e.starts_with("executed:")));
    }

    #[test]
    fn l1_effector_failure_is_not_reported_as_auto_ran() {
        let spy = Spy { fail_with: Some("boom".into()), ..Spy::default() };
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let r = engine.submit(l1(), 0);
        assert_eq!(r.disposition, Disposition::Failed, "a failed run must not read as success");
        assert!(spy.events().iter().any(|e| e.starts_with("failed:")));
        assert!(!spy.events().iter().any(|e| e.starts_with("executed:")));
    }

    #[test]
    fn l2_awaits_then_confirm_executes() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let r = engine.submit(l2(), 1000);
        assert_eq!(r.disposition, Disposition::AwaitingConfirm);
        assert_eq!(engine.pending_len(), 1);
        // no execution yet
        assert!(!spy.events().iter().any(|e| e.starts_with("executed:")));
        // confirm within the window runs it
        let out = engine.confirm(r.id, 2000);
        assert_eq!(out, Outcome::Executed);
        assert_eq!(engine.pending_len(), 0);
    }

    #[test]
    fn l2_confirm_after_timeout_is_expired_not_run() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let r = engine.submit(l2(), 0);
        let out = engine.confirm(r.id, 6000); // 6000 > 5000 timeout
        assert_eq!(out, Outcome::Expired);
        assert!(spy.events().iter().any(|e| e.starts_with("expired:")));
        assert!(!spy.events().iter().any(|e| e.starts_with("executed:")));
    }

    #[test]
    fn l2_cancel_drops_without_running() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let r = engine.submit(l2(), 0);
        let out = engine.cancel(r.id);
        assert_eq!(out, Outcome::Cancelled);
        assert_eq!(engine.pending_len(), 0);
        assert!(spy.events().iter().any(|e| e.starts_with("cancelled:")));
    }

    #[test]
    fn l3_send_is_rejected_never_run() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let r = engine.submit(l3(), 0);
        assert_eq!(
            r.disposition,
            Disposition::Rejected(RejectReason::ExternalSendNotAvailable)
        );
        assert_eq!(engine.pending_len(), 0);
        // never handed to the effector
        assert!(!spy.events().iter().any(|e| e.starts_with("run:")));
        assert!(spy.events().iter().any(|e| e.starts_with("rejected:")));
    }

    #[test]
    fn effector_failure_is_reported() {
        let spy = Spy { fail_with: Some("no such app".into()), ..Default::default() };
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        engine.submit(l1(), 0);
        assert!(spy.events().iter().any(|e| e == "failed:1:no such app"));
    }

    #[test]
    fn expire_due_sweeps_only_stale_pending() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        let a = engine.submit(l2(), 0); // submitted at 0
        let b = engine.submit(l2(), 4000); // submitted at 4000
        // at t=6000: a is 6000ms old (>5000, stale); b is 2000ms old (live)
        let expired = engine.expire_due(6000);
        assert_eq!(expired, vec![a.id]);
        assert_eq!(engine.pending_len(), 1);
        // b still confirmable
        assert_eq!(engine.confirm(b.id, 6500), Outcome::Executed);
    }

    #[test]
    fn confirm_or_cancel_unknown_id_is_unknown() {
        let spy = Spy::default();
        let mut engine = ExecutionEngine::new(&spy, &spy, 5000);
        assert_eq!(engine.confirm(ActionId(999), 0), Outcome::Unknown);
        assert_eq!(engine.cancel(ActionId(999)), Outcome::Unknown);
    }
}
