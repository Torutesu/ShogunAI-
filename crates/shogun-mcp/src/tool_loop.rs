//! The tool conversation loop (issue #81 steps 2–3, spec §4-3).
//!
//! Between the model asking for a tool and a service answering sits exactly one decision point,
//! and this is it: every `tool_use` is resolved through the catalog, judged by
//! [`crate::service_gate::authorize_op`], and only then handed to a runner. Nothing else can
//! reach a service on the model's behalf.
//!
//! **Reads run; sends only ever become proposals.** A read is executed and its data returned. A
//! [`ToolKind::Propose`] tool never performs anything here: it becomes an L3 [`Action`] handed to
//! the approval sink, and the model is told plainly that it is waiting for the user. Nothing in
//! this loop can execute a send, and no result is ever fabricated — a made-up `tool_result` is
//! worse than an error, because the model builds an answer on it.
//!
//! **Synchronous and seam-driven.** The loop is pure orchestration over two traits, so the whole
//! of it — budgets, refusals, timeouts, termination — is testable on Linux with no runtime and no
//! network. The real model and transport are async; the adapter blocks on them the same way the
//! Dream Cycle does, which keeps the async boundary at the shell rather than in the policy.
//!
//! **Where it lives.** The spec places this in `shogun-agents`; it is here because the dependency
//! runs `shogun-mcp → shogun-agents` and the loop needs the gate, the catalog and the permission
//! model at once. Same reason `tests/invariant4.rs` lives in this crate.

use serde_json::Value;

use crate::connection::ConnState;
use crate::scope::{self, OpClass, Service};
use crate::service_gate::{authorize_op, DenyReason, OpContext};
use crate::tool_catalog::{catalog_entry, proposed_action, proposed_body, ServiceState, ToolContext, ToolKind};
use shogun_agents::permission::Action;

/// Tool calls allowed in one turn (spec §4-3). Past this the model is asked to answer with what
/// it has: an unbounded loop is both a cost leak and a way for a confused model to never finish.
pub const MAX_TOOL_USES: u32 = 8;

/// Wall-clock budget for a single tool call. Exceeding it is an error handed back to the model,
/// never the end of the conversation.
pub const TOOL_TIMEOUT_MS: u64 = 10_000;

/// Byte ceiling for one tool result handed back to the model (spec §7: a large mail thread or
/// file must not blow the context window mid-loop, and the loop can carry a full tool budget of
/// results at once). An oversized result is cut and the model is told it can narrow the query,
/// rather than being handed a silently shortened answer to build on.
// ponytail: flat 16 KiB truncation on a char boundary; proper normalization/compression is #63.
pub const MAX_TOOL_RESULT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct LoopLimits {
    pub max_tool_uses: u32,
    pub tool_timeout_ms: u64,
}

impl Default for LoopLimits {
    fn default() -> Self {
        Self { max_tool_uses: MAX_TOOL_USES, tool_timeout_ms: TOOL_TIMEOUT_MS }
    }
}

/// One tool call the model asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUse {
    /// The model's own id for this call; the result must carry it back unchanged.
    pub id: String,
    /// The hub operation name (never a real MCP tool name).
    pub name: String,
    pub input: Value,
}

/// What is handed back to the model for one `tool_use`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub content: String,
    /// True for refusals and failures. The model is told plainly that the call did not happen,
    /// so it can say so rather than answering as if it had data.
    pub is_error: bool,
}

/// What the model did with its turn.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelTurn {
    /// It answered. The turn is over.
    Final(String),
    /// It wants these tools run, in order (v1 executes sequentially — spec §4-3).
    ToolUses(Vec<ToolUse>),
}

/// The model seam. The implementation is the **Agent lane only** (invariant 5): the Batch /
/// Select-KK lane never carries tool definitions, so it never drives this loop.
pub trait ModelTurnSource {
    /// Continue the conversation, having appended `results` for the previous turn's calls.
    fn next_turn(&mut self, results: &[ToolResult]) -> Result<ModelTurn, String>;
}

/// Why a tool call could not produce data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRunError {
    /// The call outran [`LoopLimits::tool_timeout_ms`].
    Timeout,
    /// The service or transport failed. The string is a short reason, never response content.
    Failed(String),
}

/// The read seam: the only way this loop can reach a service. Called **after** the gate has
/// allowed the operation, never before.
pub trait ReadToolRunner {
    fn run(
        &mut self,
        service: Service,
        scope_op: &'static str,
        input: &Value,
        timeout_ms: u64,
    ) -> Result<String, ToolRunError>;
}

/// Where a proposal goes. The implementation is the L3 approval queue; this loop only hands the
/// action over and reports back, so there is no code path here that could execute one.
pub trait ProposalSink {
    /// Queue `action` for the user's approval, with the content they will see verbatim.
    /// `Err` means it could not even be queued — never that it was performed.
    fn propose(&mut self, action: Action, body: &str) -> Result<(), String>;
}

/// Why a tool call was refused before it reached a runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Not a tool we published — the model invented it.
    UnknownTool,
    /// Published, but the gate says no right now.
    Denied(DenyReason),
    /// Published, allowed, but the input did not describe a destination — an approval prompt with
    /// no recipient is not something a user could meaningfully confirm.
    NoDestination,
    /// Published, allowed, addressed — but there is no content. The approval preview shows the
    /// body verbatim (FR-AG-03), so an empty one is as unconfirmable as a missing recipient, and
    /// letting it through would put a blank send in front of the confirm button.
    NoContent,
    /// The catalog's kind and the permission table disagree about this row. Refused rather than
    /// resolved: guessing which side is right is how a send gets executed as if it were a read.
    Inconsistent,
}

impl Refusal {
    /// What the model is told. Enough to adapt (say so, or suggest connecting), never enough to
    /// describe the internals — routes, transports and plan mechanics stay out.
    pub fn model_message(&self) -> &'static str {
        match self {
            Refusal::UnknownTool => {
                "No such tool. Use only the tools listed for this conversation."
            }
            Refusal::Denied(DenyReason::NotConnected) => {
                "That service is not connected, so it cannot be read. Tell the user they can \
                 connect it in Settings."
            }
            Refusal::Denied(DenyReason::NeedsReauth) => {
                "That service needs the user to sign in again before it can be used."
            }
            Refusal::Denied(DenyReason::PlanNotEntitled) => {
                "The user's plan does not include this. Say so briefly rather than guessing."
            }
            Refusal::Denied(DenyReason::UnreleasedWave) => {
                "That service is not available yet."
            }
            Refusal::Denied(DenyReason::UnknownOp | DenyReason::NotImplemented) => {
                "That operation is not available."
            }
            Refusal::Denied(DenyReason::DraftStop) => {
                "Sending is switched off; only drafts are possible."
            }
            Refusal::NoDestination => {
                "Say who or what this is for — an approval needs a destination."
            }
            Refusal::NoContent => {
                "Write the full content first — the user approves exactly what you write."
            }
            Refusal::Inconsistent => "That operation is not available.",
        }
    }
}

/// The verdict for one `tool_use`, before anything is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallVerdict {
    /// Allowed: run this `(service, scope op)` as a read.
    Read { service: Service, scope_op: &'static str },
    /// Allowed as a *proposal*: hand this action to the approval sink. It is L3 by construction,
    /// so nothing here can execute it.
    Propose { action: Action, body: String },
    Refused(Refusal),
}

/// Resolve and authorize one tool call. Pure — no execution, no I/O — so every branch is
/// reachable in a test.
pub fn classify_call(
    name: &str,
    input: &Value,
    services: &[ServiceState],
    ctx: &ToolContext,
) -> CallVerdict {
    let Some(entry) = catalog_entry(name) else {
        return CallVerdict::Refused(Refusal::UnknownTool);
    };
    // The catalog's promise and the permission table must agree before anything else is
    // considered. A row where they disagree is refused, never reconciled.
    let class = scope::lookup(entry.service, entry.scope_op).map(|o| o.class);
    let consistent = match entry.kind {
        ToolKind::Read => class == Some(OpClass::Read),
        ToolKind::Propose => class.is_some_and(OpClass::is_external_send),
    };
    if !consistent {
        return CallVerdict::Refused(Refusal::Inconsistent);
    }
    let conn = services
        .iter()
        .find(|s| s.service == entry.service)
        .map(|s| s.conn)
        // A service the caller did not describe is not connected. Absent means absent — never a
        // permissive default.
        .unwrap_or(ConnState::Disconnected);
    // Matched on the deny arm rather than on the allow arms: a decision variant added later is
    // then allowed-by-default *only* if it is genuinely not a denial, and no arm of this match
    // can panic — the model drives this path, and a panic here would take the app with it.
    match authorize_op(
        entry.service,
        entry.scope_op,
        &OpContext {
            highest_released: ctx.highest_released,
            conn,
            draft_stop: ctx.draft_stop,
            plan: ctx.plan,
        },
    ) {
        crate::service_gate::OpDecision::Denied(reason) => {
            CallVerdict::Refused(Refusal::Denied(reason))
        }
        _ => match entry.kind {
            ToolKind::Read => CallVerdict::Read { service: entry.service, scope_op: entry.scope_op },
            ToolKind::Propose => match proposed_action(entry, input) {
                Some(action) => {
                    // The schema requires `body`, and the guard mirrors the destination's: an
                    // approval the user cannot read is as unconfirmable as one with no recipient.
                    let body = proposed_body(input);
                    if body.trim().is_empty() {
                        CallVerdict::Refused(Refusal::NoContent)
                    } else {
                        CallVerdict::Propose { action, body }
                    }
                }
                None => CallVerdict::Refused(Refusal::NoDestination),
            },
        },
    }
}

/// What the turn produced.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopOutcome {
    /// The model's answer.
    pub answer: String,
    /// Tool calls actually executed (refusals do not spend budget — a refused call cost no
    /// service round-trip, and charging for it would let a confused model exhaust the turn
    /// without ever reaching data).
    pub executed: u32,
    /// Actions queued for the user's approval. None of them ran.
    pub proposed: u32,
    /// The tool budget ran out and the model was asked to answer with what it had.
    pub hit_tool_budget: bool,
}

/// Run one turn to completion (spec §4-3).
///
/// Errors are reserved for the conversation itself failing — a tool failing is a `tool_result`,
/// not the end of the turn.
pub fn run_read_loop<M: ModelTurnSource, R: ReadToolRunner, P: ProposalSink>(
    model: &mut M,
    runner: &mut R,
    sink: &mut P,
    services: &[ServiceState],
    ctx: &ToolContext,
    limits: LoopLimits,
) -> Result<LoopOutcome, String> {
    let mut results: Vec<ToolResult> = Vec::new();
    let mut executed = 0u32;
    let mut proposed = 0u32;
    let mut hit_tool_budget = false;

    // The tool budget alone does not terminate anything: a model that keeps asking for tools
    // after the budget is spent would be handed "budget spent" forever. A few turns past the
    // caller's actual budget — not the default const, or a raised budget punishes a well-behaved
    // model — is the backstop that makes termination a property of the loop rather than a hope
    // about the model.
    let max_model_turns = limits.max_tool_uses.saturating_add(3);
    for _ in 0..max_model_turns {
        match model.next_turn(&results)? {
            ModelTurn::Final(answer) => {
                return Ok(LoopOutcome { answer, executed, proposed, hit_tool_budget })
            }
            ModelTurn::ToolUses(uses) => {
                results = uses
                    .into_iter()
                    .map(|use_| {
                        if executed + proposed >= limits.max_tool_uses {
                            hit_tool_budget = true;
                            return ToolResult {
                                id: use_.id,
                                content: "Tool budget for this turn is spent. Answer with what \
                                          you have so far."
                                    .to_string(),
                                is_error: true,
                            };
                        }
                        match classify_call(&use_.name, &use_.input, services, ctx) {
                            CallVerdict::Refused(r) => ToolResult {
                                id: use_.id,
                                content: r.model_message().to_string(),
                                is_error: true,
                            },
                            // A proposal is not a result. It is flagged as an error so the model
                            // cannot report the send as done, and the text says what actually
                            // happened: it is waiting for the user. Only a queued proposal counts
                            // — a failed enqueue put nothing in front of the user, so it must not
                            // be reported as waiting, and it spends no budget (same rule as
                            // refusals above).
                            CallVerdict::Propose { action, body } => {
                                match sink.propose(action, &body) {
                                    Ok(()) => {
                                        proposed += 1;
                                        ToolResult {
                                            id: use_.id,
                                            content: "Prepared and waiting for the user's \
                                                      approval. Nothing has been sent. Tell them \
                                                      it is ready to approve."
                                                .to_string(),
                                            is_error: true,
                                        }
                                    }
                                    Err(why) => ToolResult {
                                        id: use_.id,
                                        content: format!(
                                            "That could not be prepared ({why}). Nothing has been \
                                             sent."
                                        ),
                                        is_error: true,
                                    },
                                }
                            }
                            CallVerdict::Read { service, scope_op } => {
                                executed += 1;
                                match runner.run(service, scope_op, &use_.input, limits.tool_timeout_ms) {
                                    Ok(content) => ToolResult {
                                        id: use_.id,
                                        content: truncate_result(content),
                                        is_error: false,
                                    },
                                    // A failure keeps the conversation alive: the model is told
                                    // this one call did not answer, and can try another way.
                                    Err(ToolRunError::Timeout) => ToolResult {
                                        id: use_.id,
                                        content: "That took too long and was stopped.".to_string(),
                                        is_error: true,
                                    },
                                    Err(ToolRunError::Failed(why)) => ToolResult {
                                        id: use_.id,
                                        content: format!("That could not be read ({why})."),
                                        is_error: true,
                                    },
                                }
                            }
                        }
                    })
                    .collect();
            }
        }
    }
    // The model never settled. Ending the turn beats spinning: the caller shows what it has —
    // and anything already queued for approval is named, because a give-up that discards the
    // count would leave the user with pending approvals nobody told them about.
    Err(if proposed > 0 {
        format!(
            "the model kept asking for tools without answering; {proposed} proposed action(s) \
             are already waiting for the user's approval"
        )
    } else {
        "the model kept asking for tools without answering".to_string()
    })
}

/// Cut an oversized result at [`MAX_TOOL_RESULT_BYTES`], always on a char boundary — this product
/// carries Japanese text, and a byte-index slice would panic mid-codepoint — with a marker that
/// tells the model the result was cut and it can narrow the query.
fn truncate_result(mut content: String) -> String {
    if content.len() <= MAX_TOOL_RESULT_BYTES {
        return content;
    }
    let mut end = MAX_TOOL_RESULT_BYTES;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);
    content.push_str("\n[Result truncated. Narrow the query to see the rest.]");
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::Wave;
    use serde_json::json;
    use shogun_agents::entitlement::{entitlements, Plan};

    fn ctx() -> ToolContext {
        ToolContext {
            highest_released: Wave::One,
            draft_stop: true,
            plan: entitlements(Plan::Pro, 0),
        }
    }

    fn connected(service: Service) -> ServiceState {
        ServiceState { service, conn: ConnState::Connected { last_sync_ms: 0 } }
    }

    fn use_(id: &str, name: &str) -> ToolUse {
        ToolUse { id: id.into(), name: name.into(), input: json!({}) }
    }

    /// A model that replays a fixed script of turns and records what it was handed back.
    struct ScriptedModel {
        turns: Vec<ModelTurn>,
        seen: Vec<Vec<ToolResult>>,
    }

    impl ScriptedModel {
        fn new(turns: Vec<ModelTurn>) -> Self {
            Self { turns, seen: Vec::new() }
        }
    }

    impl ModelTurnSource for ScriptedModel {
        fn next_turn(&mut self, results: &[ToolResult]) -> Result<ModelTurn, String> {
            self.seen.push(results.to_vec());
            if self.turns.is_empty() {
                return Err("script exhausted".into());
            }
            Ok(self.turns.remove(0))
        }
    }

    /// A runner that records every call and answers with a canned outcome.
    struct RecordingRunner {
        calls: Vec<(Service, &'static str, u64)>,
        answer: Result<String, ToolRunError>,
    }

    impl RecordingRunner {
        fn ok() -> Self {
            Self { calls: Vec::new(), answer: Ok("2 events tomorrow".into()) }
        }
        fn failing(e: ToolRunError) -> Self {
            Self { calls: Vec::new(), answer: Err(e) }
        }
    }

    impl ReadToolRunner for RecordingRunner {
        fn run(
            &mut self,
            service: Service,
            scope_op: &'static str,
            _input: &Value,
            timeout_ms: u64,
        ) -> Result<String, ToolRunError> {
            self.calls.push((service, scope_op, timeout_ms));
            self.answer.clone()
        }
    }

    /// Records everything queued for approval, and can refuse.
    #[derive(Default)]
    struct RecordingSink {
        queued: Vec<(Action, String)>,
        refuse: Option<String>,
    }

    impl ProposalSink for RecordingSink {
        fn propose(&mut self, action: Action, body: &str) -> Result<(), String> {
            if let Some(why) = &self.refuse {
                return Err(why.clone());
            }
            self.queued.push((action, body.to_string()));
            Ok(())
        }
    }

    /// A model that asks for one tool forever — the termination backstop's test subject.
    struct NeverSettles;
    impl ModelTurnSource for NeverSettles {
        fn next_turn(&mut self, _results: &[ToolResult]) -> Result<ModelTurn, String> {
            Ok(ModelTurn::ToolUses(vec![use_("x", "list_calendar_events")]))
        }
    }

    // ---- classification ----------------------------------------------------------------------

    #[test]
    fn a_published_tool_on_a_connected_service_is_allowed() {
        let v = classify_call("list_calendar_events", &json!({}), &[connected(Service::GoogleCalendar)], &ctx());
        assert_eq!(
            v,
            CallVerdict::Read { service: Service::GoogleCalendar, scope_op: "read_sync" }
        );
    }

    #[test]
    fn a_hallucinated_tool_is_refused_without_touching_a_service() {
        let v = classify_call("delete_everything", &json!({}), &[connected(Service::GoogleCalendar)], &ctx());
        assert_eq!(v, CallVerdict::Refused(Refusal::UnknownTool));
    }

    #[test]
    fn a_service_the_caller_did_not_describe_counts_as_disconnected() {
        // Absent must never mean permitted: the loop is handed the state it is allowed to trust.
        let v = classify_call("list_calendar_events", &json!({}), &[], &ctx());
        assert_eq!(v, CallVerdict::Refused(Refusal::Denied(DenyReason::NotConnected)));
    }

    #[test]
    fn the_gate_reason_survives_to_the_model_without_leaking_internals() {
        let v = classify_call(
            "list_calendar_events",
            &json!({}),
            &[ServiceState { service: Service::GoogleCalendar, conn: ConnState::Disconnected }],
            &ctx(),
        );
        let CallVerdict::Refused(r) = v else { panic!("expected a refusal") };
        let msg = r.model_message();
        assert!(msg.contains("not connected"));
        for leak in ["Composio", "MCP", "gate", "scope", "wave", "transport"] {
            assert!(!msg.contains(leak), "refusal leaks {leak}: {msg}");
        }
    }

    #[test]
    fn every_refusal_has_a_message_that_says_no_data_came_back() {
        // Each variant must be answerable; a silent refusal would let the model assume success.
        let reasons = [
            Refusal::UnknownTool,
            Refusal::NoDestination,
            Refusal::NoContent,
            Refusal::Inconsistent,
            Refusal::Denied(DenyReason::NotConnected),
            Refusal::Denied(DenyReason::NeedsReauth),
            Refusal::Denied(DenyReason::PlanNotEntitled),
            Refusal::Denied(DenyReason::UnreleasedWave),
            Refusal::Denied(DenyReason::UnknownOp),
            Refusal::Denied(DenyReason::NotImplemented),
            Refusal::Denied(DenyReason::DraftStop),
        ];
        for r in reasons {
            assert!(!r.model_message().is_empty(), "{r:?} has no message");
        }
    }

    // ---- the loop ----------------------------------------------------------------------------

    #[test]
    fn a_read_result_is_handed_back_and_the_answer_returned() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("t1", "list_calendar_events")]),
            ModelTurn::Final("You have two meetings tomorrow.".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();

        assert_eq!(out.answer, "You have two meetings tomorrow.");
        assert_eq!(out.executed, 1);
        assert!(!out.hit_tool_budget);
        // The runner saw the mapped scope op and the timeout budget.
        assert_eq!(runner.calls, vec![(Service::GoogleCalendar, "read_sync", TOOL_TIMEOUT_MS)]);
        // …and the model got the content back against its own call id.
        assert_eq!(
            model.seen[1],
            vec![ToolResult {
                id: "t1".into(),
                content: "2 events tomorrow".into(),
                is_error: false
            }]
        );
    }

    #[test]
    fn a_refused_call_never_reaches_the_runner() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("t1", "search_mail")]),
            ModelTurn::Final("Mail isn't connected.".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        // Calendar is connected; mail is not described at all.
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();

        assert!(runner.calls.is_empty(), "a refusal must not reach a service");
        assert_eq!(out.executed, 0, "a refusal spends no budget");
        assert!(model.seen[1][0].is_error);
    }

    #[test]
    fn a_timeout_is_an_error_result_not_the_end_of_the_turn() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("t1", "list_calendar_events")]),
            ModelTurn::Final("I couldn't reach your calendar in time.".into()),
        ]);
        let mut runner = RecordingRunner::failing(ToolRunError::Timeout);
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();

        assert_eq!(out.answer, "I couldn't reach your calendar in time.");
        assert!(model.seen[1][0].is_error);
        assert!(model.seen[1][0].content.contains("too long"));
    }

    #[test]
    fn a_transport_failure_never_becomes_a_fabricated_result() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("t1", "list_calendar_events")]),
            ModelTurn::Final("done".into()),
        ]);
        let mut runner = RecordingRunner::failing(ToolRunError::Failed("http 503".into()));
        run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();
        let result = &model.seen[1][0];
        assert!(result.is_error, "a failure must be flagged, never passed off as data");
        assert!(result.content.contains("could not be read"));
    }

    #[test]
    fn the_tool_budget_stops_execution_and_asks_for_an_answer() {
        let limits = LoopLimits { max_tool_uses: 2, tool_timeout_ms: TOOL_TIMEOUT_MS };
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("a", "list_calendar_events")]),
            ModelTurn::ToolUses(vec![use_("b", "list_calendar_events")]),
            ModelTurn::ToolUses(vec![use_("c", "list_calendar_events")]),
            ModelTurn::Final("Here's what I found.".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            limits,
        )
        .unwrap();

        assert_eq!(out.executed, 2, "the budget is a hard ceiling");
        assert!(out.hit_tool_budget);
        assert_eq!(runner.calls.len(), 2);
        let over_budget = &model.seen[3][0];
        assert!(over_budget.is_error);
        assert!(over_budget.content.contains("Answer with what you have"));
    }

    #[test]
    fn a_model_that_never_answers_terminates_instead_of_spinning() {
        let mut runner = RecordingRunner::ok();
        let err = run_read_loop(
            &mut NeverSettles,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap_err();
        assert!(err.contains("without answering"));
        // Termination is the loop's own property: the tool budget bounded the service calls, and
        // the model-turn backstop bounded everything else.
        assert!(runner.calls.len() as u32 <= MAX_TOOL_USES);
    }

    #[test]
    fn tool_uses_in_one_turn_run_in_order() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![
                use_("t1", "list_calendar_events"),
                use_("t2", "check_calendar_availability"),
            ]),
            ModelTurn::Final("ok".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();
        assert_eq!(
            runner.calls,
            vec![
                (Service::GoogleCalendar, "read_sync", TOOL_TIMEOUT_MS),
                (Service::GoogleCalendar, "free_busy", TOOL_TIMEOUT_MS),
            ]
        );
        let ids: Vec<&str> = model.seen[1].iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["t1", "t2"], "results come back against their own ids, in order");
    }

    // ---- proposals ---------------------------------------------------------------------------

    #[test]
    fn a_send_becomes_a_proposal_and_is_never_executed() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![ToolUse {
                id: "p1".into(),
                name: "propose_calendar_event".into(),
                input: json!({ "title": "Vendor sync", "body": "Tuesday 3pm with Acme" }),
            }]),
            ModelTurn::Final("I've prepared it for you to approve.".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let mut sink = RecordingSink::default();
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut sink,
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();

        assert!(runner.calls.is_empty(), "a proposal must never reach the read runner");
        assert_eq!(out.executed, 0);
        assert_eq!(out.proposed, 1);
        assert_eq!(
            sink.queued,
            vec![(
                Action::Send(shogun_agents::permission::SendAction::CreateCalendarEvent {
                    title: "Vendor sync".into()
                }),
                "Tuesday 3pm with Acme".to_string()
            )]
        );
        // The model is told it is waiting, and told as an error so it cannot report it as done.
        let result = &model.seen[1][0];
        assert!(result.is_error);
        assert!(result.content.contains("Nothing has been sent"));
    }

    #[test]
    fn a_proposal_the_queue_refuses_is_still_not_a_send() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![ToolUse {
                id: "p1".into(),
                name: "propose_calendar_event".into(),
                input: json!({ "title": "Sync", "body": "b" }),
            }]),
            ModelTurn::Final("I couldn't prepare it.".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let mut sink = RecordingSink { queued: Vec::new(), refuse: Some("queue full".into()) };
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut sink,
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();
        let result = &model.seen[1][0];
        assert!(result.is_error);
        assert!(result.content.contains("Nothing has been sent"));
        assert!(runner.calls.is_empty());
        // Nothing reached the queue, so nothing may be reported as waiting: a caller rendering
        // "1 action waiting" from this count would send the user to an empty queue.
        assert_eq!(out.proposed, 0);
    }

    #[test]
    fn a_proposal_without_a_destination_is_refused_before_the_queue() {
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![ToolUse {
                id: "p1".into(),
                name: "propose_send_email".into(),
                input: json!({ "body": "hello" }),
            }]),
            ModelTurn::Final("Who should I send it to?".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let mut sink = RecordingSink::default();
        // Gmail connected, draft-stop OFF so the gate itself would allow the send.
        let ctx = ToolContext {
            highest_released: Wave::One,
            draft_stop: false,
            plan: entitlements(Plan::Pro, 0),
        };
        run_read_loop(
            &mut model,
            &mut runner,
            &mut sink,
            &[connected(Service::Gmail)],
            &ctx,
            LoopLimits::default(),
        )
        .unwrap();
        assert!(sink.queued.is_empty(), "an approval with no recipient must not be queued");
        assert!(model.seen[1][0].content.contains("destination"));
    }

    #[test]
    fn a_proposal_without_content_is_refused_before_the_queue() {
        // `body` is required by the schema; a model omitting it is off-spec. The same argument as
        // the destination guard: an approval showing nothing is not something a user could
        // meaningfully confirm, so it must never reach the queue.
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![ToolUse {
                id: "p1".into(),
                name: "propose_calendar_event".into(),
                input: json!({ "title": "Vendor sync", "body": "   " }),
            }]),
            ModelTurn::Final("Let me write it out first.".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let mut sink = RecordingSink::default();
        run_read_loop(
            &mut model,
            &mut runner,
            &mut sink,
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();
        assert!(sink.queued.is_empty(), "an approval with no content must not be queued");
        // A body that is absent entirely is the same refusal as a blank one.
        assert_eq!(
            classify_call(
                "propose_calendar_event",
                &json!({ "title": "t" }),
                &[connected(Service::GoogleCalendar)],
                &ctx(),
            ),
            CallVerdict::Refused(Refusal::NoContent)
        );
        let result = &model.seen[1][0];
        assert!(result.is_error);
        assert!(result.content.contains("content"), "{}", result.content);
        for leak in ["Composio", "MCP", "gate", "scope", "route", "transport", "plan"] {
            assert!(!result.content.contains(leak), "refusal leaks {leak}: {}", result.content);
        }
    }

    #[test]
    fn draft_stop_blocks_the_email_proposal_at_the_gate() {
        // §6.10: with draft-stop on (the default), Gmail send is refused — the model cannot even
        // queue it for approval.
        let v = classify_call(
            "propose_send_email",
            &json!({ "to": "a@b.com", "body": "hi" }),
            &[connected(Service::Gmail)],
            &ctx(),
        );
        assert_eq!(v, CallVerdict::Refused(Refusal::Denied(DenyReason::DraftStop)));
    }

    #[test]
    fn proposals_and_reads_share_one_budget() {
        let limits = LoopLimits { max_tool_uses: 2, tool_timeout_ms: TOOL_TIMEOUT_MS };
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("r1", "list_calendar_events")]),
            ModelTurn::ToolUses(vec![ToolUse {
                id: "p1".into(),
                name: "propose_calendar_event".into(),
                input: json!({ "title": "t", "body": "b" }),
            }]),
            ModelTurn::ToolUses(vec![use_("r2", "list_calendar_events")]),
            ModelTurn::Final("done".into()),
        ]);
        let mut runner = RecordingRunner::ok();
        let mut sink = RecordingSink::default();
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut sink,
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            limits,
        )
        .unwrap();
        assert_eq!(out.executed, 1);
        assert_eq!(out.proposed, 1);
        assert!(out.hit_tool_budget, "the third call is over the shared ceiling");
        assert_eq!(runner.calls.len(), 1);
    }

    #[test]
    fn a_raised_tool_budget_raises_the_termination_backstop_with_it() {
        // The backstop derives from the caller's actual limit, not the default const: a caller
        // raising the budget must not get "never answered" from a model that answers the moment
        // its budget is spent.
        let limits = LoopLimits { max_tool_uses: 20, tool_timeout_ms: TOOL_TIMEOUT_MS };
        let mut turns: Vec<ModelTurn> = (0..20)
            .map(|i| ModelTurn::ToolUses(vec![use_(&format!("t{i}"), "list_calendar_events")]))
            .collect();
        turns.push(ModelTurn::Final("done".into()));
        let mut model = ScriptedModel::new(turns);
        let mut runner = RecordingRunner::ok();
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            limits,
        )
        .unwrap();
        assert_eq!(out.executed, 20, "the raised budget is honoured in full");
        assert_eq!(out.answer, "done");
        assert!(!out.hit_tool_budget);
    }

    /// A model that proposes forever — the give-up path with approvals already queued.
    struct NeverSettlesProposing;
    impl ModelTurnSource for NeverSettlesProposing {
        fn next_turn(&mut self, _results: &[ToolResult]) -> Result<ModelTurn, String> {
            Ok(ModelTurn::ToolUses(vec![ToolUse {
                id: "p".into(),
                name: "propose_calendar_event".into(),
                input: json!({ "title": "t", "body": "b" }),
            }]))
        }
    }

    #[test]
    fn the_give_up_path_names_the_proposals_it_leaves_queued() {
        // The give-up discards the turn, but the queued proposals are real: the error must say
        // they are waiting, or the user ends up with pending approvals nobody told them about.
        let mut sink = RecordingSink::default();
        let err = run_read_loop(
            &mut NeverSettlesProposing,
            &mut RecordingRunner::ok(),
            &mut sink,
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap_err();
        assert!(err.contains("without answering"));
        assert_eq!(sink.queued.len() as u32, MAX_TOOL_USES, "the budget bounded the queueing");
        assert!(err.contains(&sink.queued.len().to_string()), "the count is missing: {err}");
    }

    #[test]
    fn an_oversized_multibyte_result_is_truncated_on_a_char_boundary() {
        // Japanese text is 3 bytes per char, so the raw ceiling lands mid-codepoint: the cut must
        // land on a boundary or the string is corrupt (and a byte slice would panic).
        let big = "あ".repeat(MAX_TOOL_RESULT_BYTES); // 3× the ceiling in bytes
        let mut model = ScriptedModel::new(vec![
            ModelTurn::ToolUses(vec![use_("t1", "list_calendar_events")]),
            ModelTurn::Final("done".into()),
        ]);
        let mut runner = RecordingRunner { calls: Vec::new(), answer: Ok(big) };
        run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[connected(Service::GoogleCalendar)],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();
        let result = &model.seen[1][0];
        assert!(!result.is_error, "truncation is not a failure");
        assert!(result.content.len() < MAX_TOOL_RESULT_BYTES + 100, "not actually truncated");
        assert!(result.content.contains("truncated"), "the model must be told it was cut");
        let body = result.content.split('\n').next().unwrap_or_default();
        assert!(!body.is_empty() && body.chars().all(|c| c == 'あ'), "the cut corrupted the text");
    }

    #[test]
    fn a_result_under_the_ceiling_is_passed_through_untouched() {
        assert_eq!(truncate_result("2 events tomorrow".into()), "2 events tomorrow");
    }

    #[test]
    fn an_answer_with_no_tools_at_all_is_a_normal_turn() {
        let mut model = ScriptedModel::new(vec![ModelTurn::Final("Nothing to look up.".into())]);
        let mut runner = RecordingRunner::ok();
        let out = run_read_loop(
            &mut model,
            &mut runner,
            &mut RecordingSink::default(),
            &[],
            &ctx(),
            LoopLimits::default(),
        )
        .unwrap();
        assert_eq!(out.executed, 0);
        assert!(runner.calls.is_empty());
        assert_eq!(out.answer, "Nothing to look up.");
    }
}
