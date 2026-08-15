//! The read-tool conversation loop (issue #81 step 2, spec §4-3).
//!
//! Between the model asking for a tool and a service answering sits exactly one decision point,
//! and this is it: every `tool_use` is resolved through the catalog, judged by
//! [`crate::service_gate::authorize_op`], and only then handed to a runner. Nothing else can
//! reach a service on the model's behalf.
//!
//! **Reads only.** A tool the catalog does not publish as a read is refused here rather than
//! executed, and the refusal says so honestly instead of inventing a result — a fabricated
//! `tool_result` is worse than an error, because the model will build an answer on it. Writes and
//! sends route through the L1/L2/L3 engine, which is step 3.
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
use crate::tool_catalog::{catalog_entry, ServiceState, ToolContext};

/// Tool calls allowed in one turn (spec §4-3). Past this the model is asked to answer with what
/// it has: an unbounded loop is both a cost leak and a way for a confused model to never finish.
pub const MAX_TOOL_USES: u32 = 8;

/// Wall-clock budget for a single tool call. Exceeding it is an error handed back to the model,
/// never the end of the conversation.
pub const TOOL_TIMEOUT_MS: u64 = 10_000;

/// How many model turns may pass before the loop gives up regardless of the tool budget.
///
/// The tool budget alone does not terminate anything: a model that keeps asking for tools after
/// the budget is spent would be handed "budget spent" forever. This is the backstop that makes
/// termination a property of the loop rather than a hope about the model.
const MAX_MODEL_TURNS: u32 = MAX_TOOL_USES + 3;

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

/// Why a tool call was refused before it reached a runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Not a tool we published — the model invented it.
    UnknownTool,
    /// Published, but the gate says no right now.
    Denied(DenyReason),
    /// Published but not a read. Unreachable while the catalog holds reads only; kept as a hard
    /// stop so a future catalog row cannot make this loop execute a write.
    NotARead,
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
            Refusal::NotARead => {
                "That action needs the user's approval and cannot be run from here."
            }
        }
    }
}

/// The verdict for one `tool_use`, before anything is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallVerdict {
    /// Allowed: run this `(service, scope op)` as a read.
    Read { service: Service, scope_op: &'static str },
    Refused(Refusal),
}

/// Resolve and authorize one tool call. Pure — no execution, no I/O — so every branch is
/// reachable in a test.
pub fn classify_call(name: &str, services: &[ServiceState], ctx: &ToolContext) -> CallVerdict {
    let Some(entry) = catalog_entry(name) else {
        return CallVerdict::Refused(Refusal::UnknownTool);
    };
    // A catalog row that is not a read must never be executed here, whatever the gate says about
    // it — the approval engine owns those (invariant 4).
    if scope::lookup(entry.service, entry.scope_op).map(|o| o.class) != Some(OpClass::Read) {
        return CallVerdict::Refused(Refusal::NotARead);
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
        _ => CallVerdict::Read { service: entry.service, scope_op: entry.scope_op },
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
    /// The tool budget ran out and the model was asked to answer with what it had.
    pub hit_tool_budget: bool,
}

/// Run one turn to completion (spec §4-3).
///
/// Errors are reserved for the conversation itself failing — a tool failing is a `tool_result`,
/// not the end of the turn.
pub fn run_read_loop<M: ModelTurnSource, R: ReadToolRunner>(
    model: &mut M,
    runner: &mut R,
    services: &[ServiceState],
    ctx: &ToolContext,
    limits: LoopLimits,
) -> Result<LoopOutcome, String> {
    let mut results: Vec<ToolResult> = Vec::new();
    let mut executed = 0u32;
    let mut hit_tool_budget = false;

    for _ in 0..MAX_MODEL_TURNS {
        match model.next_turn(&results)? {
            ModelTurn::Final(answer) => {
                return Ok(LoopOutcome { answer, executed, hit_tool_budget })
            }
            ModelTurn::ToolUses(uses) => {
                results = uses
                    .into_iter()
                    .map(|use_| {
                        if executed >= limits.max_tool_uses {
                            hit_tool_budget = true;
                            return ToolResult {
                                id: use_.id,
                                content: "Tool budget for this turn is spent. Answer with what \
                                          you have so far."
                                    .to_string(),
                                is_error: true,
                            };
                        }
                        match classify_call(&use_.name, services, ctx) {
                            CallVerdict::Refused(r) => ToolResult {
                                id: use_.id,
                                content: r.model_message().to_string(),
                                is_error: true,
                            },
                            CallVerdict::Read { service, scope_op } => {
                                executed += 1;
                                match runner.run(service, scope_op, &use_.input, limits.tool_timeout_ms) {
                                    Ok(content) => {
                                        ToolResult { id: use_.id, content, is_error: false }
                                    }
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
    // The model never settled. Ending the turn beats spinning: the caller shows what it has.
    Err("the model kept asking for tools without answering".to_string())
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
        let v = classify_call("list_calendar_events", &[connected(Service::GoogleCalendar)], &ctx());
        assert_eq!(
            v,
            CallVerdict::Read { service: Service::GoogleCalendar, scope_op: "read_sync" }
        );
    }

    #[test]
    fn a_hallucinated_tool_is_refused_without_touching_a_service() {
        let v = classify_call("delete_everything", &[connected(Service::GoogleCalendar)], &ctx());
        assert_eq!(v, CallVerdict::Refused(Refusal::UnknownTool));
    }

    #[test]
    fn a_service_the_caller_did_not_describe_counts_as_disconnected() {
        // Absent must never mean permitted: the loop is handed the state it is allowed to trust.
        let v = classify_call("list_calendar_events", &[], &ctx());
        assert_eq!(v, CallVerdict::Refused(Refusal::Denied(DenyReason::NotConnected)));
    }

    #[test]
    fn the_gate_reason_survives_to_the_model_without_leaking_internals() {
        let v = classify_call(
            "list_calendar_events",
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
            Refusal::NotARead,
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

    #[test]
    fn an_answer_with_no_tools_at_all_is_a_normal_turn() {
        let mut model = ScriptedModel::new(vec![ModelTurn::Final("Nothing to look up.".into())]);
        let mut runner = RecordingRunner::ok();
        let out = run_read_loop(&mut model, &mut runner, &[], &ctx(), LoopLimits::default()).unwrap();
        assert_eq!(out.executed, 0);
        assert!(runner.calls.is_empty());
        assert_eq!(out.answer, "Nothing to look up.");
    }
}
