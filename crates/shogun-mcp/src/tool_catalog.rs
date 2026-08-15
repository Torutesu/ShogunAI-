//! What the model is allowed to see: the LLM-facing tool catalog and the "Connected services"
//! system-prompt block (issue #81 step 1, `docs/mcp/01-architecture.md` §5).
//!
//! Two rules shape this module, and both are structural rather than advisory:
//!
//! 1. **The model never sees a real MCP tool name.** Every entry is a *hub operation name*
//!    (`list_calendar_events`), bound to a `(Service, scope op)` pair. Real tool names live in
//!    `shogun_integrations::toolmap` and change with the servers; this layer is the stable
//!    interface, which is why re-routing Gmail from the first layer to Composio cost a toolmap
//!    edit and nothing here (§5-2).
//! 2. **An operation that is not in the permission table cannot be offered.** Every entry
//!    resolves through [`crate::scope::lookup`], and the definitions are filtered by the same
//!    [`crate::service_gate::authorize_op`] that will judge the call — so the set the model can
//!    see is a subset of the set the gate would allow. Not being able to call it is the first
//!    line of defence (§5-1); the gate is the second, and it is the one that actually holds.
//!
//! Two kinds of tool are published. A [`ToolKind::Read`] runs and returns data. A
//! [`ToolKind::Propose`] **never runs here**: it maps to a [`SendAction`], which is L3 by
//! construction, so calling it can only ever produce a proposal the user has to approve
//! (invariant 4). The model is told exactly that, so it does not report a send as done.
//!
//! Not every row of the permission table is publishable yet, and the gaps are deliberate:
//!
//! - `ServiceStateChange` rows (the Gmail draft/label writes) have **no [`Action`] to map to** —
//!   `LocalAction` is on-device only and `SendAction` leaves the device irreversibly, and a
//!   reversible write *to a service* is neither. Inventing a third category is a change to the
//!   permission model, not to this catalog, so those stay unpublished.
//! - Two sends collide on one `SendAction` variant (Linear's issue comment with GitHub's, Notion's
//!   page create with Drive's). [`shogun_integrations::send_bridge::route_send`] maps a variant to
//!   exactly one service, so publishing both would let an approved Linear comment execute against
//!   GitHub. The colliding half stays unpublished until the variant can name its service — a
//!   round-trip test in shogun-integrations holds this line.

use serde_json::{json, Value};

use crate::connection::ConnState;
use crate::scope::{self, OpClass, Service, Wave};
use crate::service_gate::{authorize_op, OpContext};
use shogun_agents::entitlement::Entitlements;
use shogun_agents::permission::{Action, SendAction};

/// What happens when the model calls a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// Runs immediately and returns data.
    Read,
    /// Never runs here: it becomes a proposal the user has to approve (invariant 4).
    Propose,
}

/// One tool as the model sees it: a hub operation name, a role-level description, and the JSON
/// Schema for its input. Bound to the `(service, scope op)` the gate will authorize.
#[derive(Debug, Clone, Copy)]
pub struct ToolEntry {
    /// The stable hub operation name the model calls (never a real MCP tool name).
    pub name: &'static str,
    pub service: Service,
    /// The row in [`crate::scope`]'s permission table this maps to. An entry whose op is absent
    /// from the table is a build-time-visible bug (test-asserted), not a silent no-op.
    pub scope_op: &'static str,
    /// What the tool is for, in the user's terms. Never mentions transports, routes, or the
    /// distinction between the first and second layer — that disclosure belongs to the UI, not
    /// to the model's judgement (§5-1).
    pub description: &'static str,
    /// Whether calling it reads, or proposes something the user must approve.
    pub kind: ToolKind,
}

/// The catalog. Read operations only, in the order they are offered.
const CATALOG: &[ToolEntry] = &[
    ToolEntry {
        name: "list_calendar_events",
        service: Service::GoogleCalendar,
        scope_op: "read_sync",
        description: "List the user's calendar events in a time range. Use this for questions \
                      about their schedule, what a meeting is, or who is attending.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "check_calendar_availability",
        service: Service::GoogleCalendar,
        scope_op: "free_busy",
        description: "Find when the user is free or busy in a time range. Use this before \
                      proposing a time, never to list what a meeting is about.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "search_mail",
        service: Service::Gmail,
        scope_op: "read_sync",
        description: "Search the user's mail for threads matching a query. Use this for \
                      questions about conversations, requests, or what someone is waiting on.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "get_mail_thread",
        service: Service::Gmail,
        scope_op: "read_on_demand",
        description: "Read one mail thread in full by its id, when a search result is not enough \
                      to answer.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "list_recent_drive_files",
        service: Service::GoogleDrive,
        scope_op: "read_sync",
        description: "List the user's recently touched documents and files.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "read_drive_file",
        service: Service::GoogleDrive,
        scope_op: "read_on_demand",
        description: "Read the contents of one document or file by its id.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "search_chat_messages",
        service: Service::Slack,
        scope_op: "read_sync",
        description: "Search the user's chat messages for a query.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "search_docs",
        service: Service::Notion,
        scope_op: "read_sync",
        description: "Search the user's notes and documents for a query.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "search_code_issues",
        service: Service::GitHub,
        scope_op: "read_sync",
        description: "Search the user's code issues and pull requests for a query.",
        kind: ToolKind::Read,
    },
    ToolEntry {
        name: "list_tracker_issues",
        service: Service::Linear,
        scope_op: "read_sync",
        description: "List issues from the user's issue tracker.",
        kind: ToolKind::Read,
    },
    // ---- proposals. Calling one of these asks the user; it never performs anything. ----------
    ToolEntry {
        name: "propose_send_email",
        service: Service::Gmail,
        scope_op: "send",
        description: "Propose sending an email. This does not send: it asks the user to approve \
                      the message first. Say that you have prepared it for their approval.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_calendar_event",
        service: Service::GoogleCalendar,
        scope_op: "event_create",
        description: "Propose creating a calendar event. This does not create it: the user \
                      approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_calendar_event_change",
        service: Service::GoogleCalendar,
        scope_op: "event_update_delete",
        description: "Propose changing or cancelling an existing calendar event. Every attendee \
                      sees the change, so the user approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_drive_document",
        service: Service::GoogleDrive,
        scope_op: "file_create",
        description: "Propose creating a document or file. The user approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_chat_message",
        service: Service::Slack,
        scope_op: "post_message",
        description: "Propose posting a message to a channel. This does not post: the user \
                      approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_chat_reaction",
        service: Service::Slack,
        scope_op: "reaction",
        description: "Propose reacting to a message. Everyone in the channel can see a reaction, \
                      so the user approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_issue_comment",
        service: Service::GitHub,
        scope_op: "issue_create_or_comment",
        description: "Propose commenting on an issue or pull request. The user approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_doc_change",
        service: Service::Notion,
        scope_op: "page_update",
        description: "Propose changing a page in the user's notes. The user approves it first.",
        kind: ToolKind::Propose,
    },
    ToolEntry {
        name: "propose_issue_status_change",
        service: Service::Linear,
        scope_op: "status_change",
        description: "Propose moving an issue to another state. The whole team sees the change, \
                      so the user approves it first.",
        kind: ToolKind::Propose,
    },
];

/// The catalog entry behind a tool name the model called, if it is one we published.
///
/// The conversation loop needs this to turn a `tool_use` back into the `(service, scope op)` the
/// gate judges; the cross-crate invariant test needs it to prove the published set never contains
/// a non-read. A name that is not in the catalog is `None` — a hallucinated tool has no mapping.
pub fn catalog_entry(name: &str) -> Option<&'static ToolEntry> {
    CATALOG.iter().find(|e| e.name == name)
}

/// A query + result-count input, the shape most read tools take.
fn query_schema(query_hint: &'static str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": query_hint },
            "limit": {
                "type": "integer",
                "description": "Maximum results to return (default 10, max 50).",
                "minimum": 1,
                "maximum": 50
            }
        },
        "required": ["query"]
    })
}

/// The JSON Schema for a tool's input. Defined here, beside the catalog, so an operation cannot
/// acquire a schema without also having a catalog row and a permission-table row.
fn input_schema(name: &str) -> Value {
    match name {
        "list_calendar_events" => json!({
            "type": "object",
            "properties": {
                "start": {
                    "type": "string",
                    "description": "Start of the range, ISO 8601 (e.g. 2026-08-16T00:00:00Z). \
                                    Defaults to now."
                },
                "end": {
                    "type": "string",
                    "description": "End of the range, ISO 8601. Defaults to 24 hours after start."
                }
            }
        }),
        "check_calendar_availability" => json!({
            "type": "object",
            "properties": {
                "start": { "type": "string", "description": "Start of the window, ISO 8601." },
                "end": { "type": "string", "description": "End of the window, ISO 8601." },
                "duration_minutes": {
                    "type": "integer",
                    "description": "Length of the slot being looked for.",
                    "minimum": 1
                }
            },
            "required": ["start", "end"]
        }),
        "search_mail" => query_schema("What to look for — sender, subject words, or topic."),
        "get_mail_thread" => json!({
            "type": "object",
            "properties": {
                "thread_id": {
                    "type": "string",
                    "description": "The thread id from a previous search result."
                }
            },
            "required": ["thread_id"]
        }),
        "list_recent_drive_files" => json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum files to return (default 10, max 50).",
                    "minimum": 1,
                    "maximum": 50
                }
            }
        }),
        "read_drive_file" => json!({
            "type": "object",
            "properties": {
                "file_id": {
                    "type": "string",
                    "description": "The file id from a previous listing or search."
                }
            },
            "required": ["file_id"]
        }),
        "search_chat_messages" => query_schema("What to look for — channel, person, or topic."),
        "search_docs" => query_schema("What to look for in the user's notes and documents."),
        "search_code_issues" => query_schema("What to look for — repository, label, or topic."),
        "list_tracker_issues" => query_schema("What to look for — project, assignee, or state."),
        // ---- proposals. Every one takes the addressing field plus the full content, because
        // ---- the approval preview shows the user exactly what would go out (FR-AG-03).
        "propose_send_email" => addressed_schema("to", "The recipient's email address."),
        "propose_calendar_event" => addressed_schema("title", "The event's title."),
        "propose_calendar_event_change" => {
            addressed_schema("title", "The title of the event to change.")
        }
        "propose_drive_document" => addressed_schema("title", "The document's name."),
        "propose_chat_message" => addressed_schema("channel", "The channel to post to."),
        "propose_chat_reaction" => addressed_schema("target", "The message to react to."),
        "propose_issue_comment" => addressed_schema("target", "The issue or pull request."),
        "propose_doc_change" => addressed_schema("title", "The page to change."),
        "propose_issue_status_change" => addressed_schema("target", "The issue to move."),
        // Unreachable for a catalog entry: `every_entry_has_a_schema` pins the two together.
        _ => json!({ "type": "object", "properties": {} }),
    }
}

/// A proposal's input: where it goes, and the full content the user will approve.
fn addressed_schema(field: &'static str, hint: &'static str) -> Value {
    json!({
        "type": "object",
        "properties": {
            field: { "type": "string", "description": hint },
            "body": {
                "type": "string",
                "description": "The full content. The user sees this exactly as written before \
                                approving, so write it as the finished thing, not a summary."
            }
        },
        "required": [field, "body"]
    })
}

/// The action a proposal tool stands for, built from the model's input.
///
/// Returns `None` when the entry is not a proposal, or the input is missing its addressing field
/// — a proposal with no destination is not something a user could meaningfully approve, and
/// inventing one would put an unintended recipient in front of the confirm button.
pub fn proposed_action(entry: &ToolEntry, input: &Value) -> Option<Action> {
    if entry.kind != ToolKind::Propose {
        return None;
    }
    let field = |name: &str| -> Option<String> {
        input.get(name).and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.trim().is_empty())
    };
    let send = match entry.name {
        "propose_send_email" => SendAction::SendEmail { to: field("to")? },
        "propose_calendar_event" => SendAction::CreateCalendarEvent { title: field("title")? },
        "propose_calendar_event_change" => {
            SendAction::UpdateCalendarEvent { title: field("title")? }
        }
        "propose_drive_document" => SendAction::CreateDocument { title: field("title")? },
        "propose_chat_message" => SendAction::PostMessage { channel: field("channel")? },
        "propose_chat_reaction" => SendAction::AddReaction { target: field("target")? },
        "propose_issue_comment" => SendAction::PostComment { target: field("target")? },
        "propose_doc_change" => SendAction::UpdateDocument { title: field("title")? },
        "propose_issue_status_change" => SendAction::ChangeIssueStatus { target: field("target")? },
        _ => return None,
    };
    Some(Action::Send(send))
}

/// The content the user will see in the approval preview, verbatim.
pub fn proposed_body(input: &Value) -> String {
    input.get("body").and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

/// The role a service plays, in the words the block and the descriptions use. Deliberately not
/// the product's name for the service: the model reasons about "the calendar", and the mapping
/// from that to a vendor is the hub's business.
fn role(service: Service) -> &'static str {
    match service {
        Service::GoogleCalendar => "calendar",
        Service::Gmail => "mail",
        Service::GoogleDrive => "drive",
        Service::Slack => "chat",
        Service::Notion => "docs",
        Service::GitHub => "code",
        Service::Linear => "issues",
    }
}

/// The one-line role description for the prompt block.
fn role_line(service: Service) -> &'static str {
    match service {
        Service::GoogleCalendar => {
            "the user's calendar. Events, availability, upcoming meetings. Read-only."
        }
        Service::Gmail => {
            "the user's mail. Threads and messages. Read-only; you may draft replies, but sending \
             always requires the user's explicit approval."
        }
        Service::GoogleDrive => "the user's documents and files. Read-only.",
        Service::Slack => {
            "the user's chat messages. Read-only; posting always requires the user's explicit \
             approval."
        }
        Service::Notion => "the user's notes and documents. Read-only.",
        Service::GitHub => "the user's code issues and pull requests. Read-only.",
        Service::Linear => "the user's issue tracker. Read-only.",
    }
}

/// The runtime facts the tool layer needs, shared across services.
#[derive(Debug, Clone, Copy)]
pub struct ToolContext {
    pub highest_released: Wave,
    pub draft_stop: bool,
    pub plan: Entitlements,
}

/// How the tool layer sees one service.
///
/// `conn` is the connection state **as the tool layer should treat it**, which is not always the
/// raw FSM state: Gmail without the Composio three-disclosure consent must be
/// [`ConnState::Disconnected`] here, because an unconsented mailbox has to be indistinguishable
/// from an unconnected one (未同意 = 未接続扱い, `docs/mcp/01-architecture.md` §5-1). Folding that
/// in at the caller keeps consent a single decision instead of a condition every reader repeats.
#[derive(Debug, Clone, Copy)]
pub struct ServiceState {
    pub service: Service,
    pub conn: ConnState,
}

/// Whether this entry may be offered to the model right now.
fn offerable(entry: &ToolEntry, conn: ConnState, ctx: &ToolContext) -> bool {
    // The entry's kind and the permission table's class must agree, or the tool would be
    // published under the wrong promise: a read that actually sends, or a proposal that quietly
    // runs. Disagreement drops the tool rather than guessing which side is right.
    let class = scope::lookup(entry.service, entry.scope_op).map(|o| o.class);
    let consistent = match entry.kind {
        ToolKind::Read => class == Some(OpClass::Read),
        ToolKind::Propose => class.is_some_and(OpClass::is_external_send),
    };
    if !consistent {
        return false;
    }
    // An amber service still serves cached reads, and the gate says so — deliberately reusing the
    // gate rather than restating its rules, so the two can never drift apart.
    authorize_op(
        entry.service,
        entry.scope_op,
        &OpContext {
            highest_released: ctx.highest_released,
            conn,
            draft_stop: ctx.draft_stop,
            plan: ctx.plan,
        },
    )
    .is_allowed()
}

/// The `tools` array for the Anthropic request, in the API's own shape.
///
/// Only operations that are connected, released, entitled, and in the permission table appear.
/// Everything else is absent rather than present-and-refused: a tool the model cannot see is one
/// it cannot hallucinate a call to.
pub fn tool_definitions(services: &[ServiceState], ctx: &ToolContext) -> Vec<Value> {
    CATALOG
        .iter()
        .filter_map(|entry| {
            let state = services.iter().find(|s| s.service == entry.service)?;
            offerable(entry, state.conn, ctx).then(|| {
                json!({
                    "name": entry.name,
                    "description": entry.description,
                    "input_schema": input_schema(entry.name),
                })
            })
        })
        .collect()
}

/// The services that have at least one offerable tool — the ones the prompt block may name.
fn offerable_services(services: &[ServiceState], ctx: &ToolContext) -> Vec<Service> {
    let mut out: Vec<Service> = Vec::new();
    for entry in CATALOG {
        let Some(state) = services.iter().find(|s| s.service == entry.service) else {
            continue;
        };
        if offerable(entry, state.conn, ctx) && !out.contains(&entry.service) {
            out.push(entry.service);
        }
    }
    out
}

/// The "Connected services" system-prompt block (§5-1), or `None` when nothing is connected —
/// in which case the prompt says nothing at all rather than announcing an empty list.
///
/// The block names roles, never tool names or transports, and always carries the sentence about
/// approval. The sentence is expectation-setting only: the guarantee is the gate.
pub fn connected_services_block(services: &[ServiceState], ctx: &ToolContext) -> Option<String> {
    let available = offerable_services(services, ctx);
    if available.is_empty() {
        return None;
    }

    let mut out = String::from("## Connected services\nYou can pull context from these connected services:\n");
    for s in &available {
        out.push_str(&format!("- {}: {}\n", role(*s), role_line(*s)));
    }

    // Priorities, emitted only for services that are actually there — a priority line for a
    // service the model cannot call is an instruction it can only fail to follow.
    let has = |s: Service| available.contains(&s);
    let mut priorities: Vec<String> = Vec::new();
    if has(Service::GoogleCalendar) {
        priorities.push(
            "- Questions about schedule, meetings, or availability → check calendar first.".into(),
        );
    }
    if has(Service::Gmail) {
        priorities.push(
            "- Questions about conversations, requests, or follow-ups → check mail first.".into(),
        );
    }
    if has(Service::GoogleDrive) {
        priorities.push("- Questions about documents or materials → check drive first.".into());
    }
    if has(Service::Slack) {
        priorities.push("- Questions about what was said in a channel → check chat first.".into());
    }
    // One combination example, never a catalogue of them: enumerating patterns makes the model
    // rigid (§5-3).
    if has(Service::GoogleCalendar) && has(Service::GoogleDrive) && has(Service::Gmail) {
        priorities.push(
            "- For meeting prep, combine: calendar (the event) → drive (related files) → mail \
             (related threads)."
                .into(),
        );
    }
    if !priorities.is_empty() {
        out.push_str("\nPriorities:\n");
        for p in priorities {
            out.push_str(&p);
            out.push('\n');
        }
    }

    out.push_str(
        "\nIf a task would clearly benefit from a service that is not listed here, say so briefly \
         instead of guessing (the user can connect it in Settings).\n",
    );
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ReauthReason;
    use shogun_agents::entitlement::{entitlements, Plan};
    use shogun_agents::permission::Level;

    /// Real plan values, not hand-assembled ones: the point of the plan filter is that it agrees
    /// with the entitlement table, and a fixture that invents its own would not test that.
    fn pro() -> Entitlements {
        entitlements(Plan::Pro, 0)
    }

    fn expired_trial() -> Entitlements {
        entitlements(Plan::Trial { started_at_ms: Some(0) }, 30 * 24 * 60 * 60 * 1000)
    }

    fn ctx() -> ToolContext {
        ToolContext { highest_released: Wave::One, draft_stop: true, plan: pro() }
    }

    fn connected(service: Service) -> ServiceState {
        ServiceState { service, conn: ConnState::Connected { last_sync_ms: 0 } }
    }

    fn names(v: &[Value]) -> Vec<String> {
        v.iter().map(|t| t["name"].as_str().unwrap_or_default().to_string()).collect()
    }

    // ---- the structural promises -------------------------------------------------------------

    #[test]
    fn every_catalog_entry_exists_in_the_permission_table() {
        // "An operation not in the table has no schema" is only true if this holds: a catalog row
        // whose op is absent would be a tool the gate can only ever deny with UnknownOp.
        for e in CATALOG {
            assert!(
                scope::lookup(e.service, e.scope_op).is_some(),
                "{} maps to {:?}/{} which is not in the permission table",
                e.name,
                e.service,
                e.scope_op
            );
        }
    }

    #[test]
    fn every_catalog_entry_matches_its_permission_class() {
        // The kind is the promise made to the model ("this returns data" / "this only asks").
        // The permission table is the truth. They must agree row by row, or a tool is published
        // under a promise it cannot keep.
        for e in CATALOG {
            let class = scope::lookup(e.service, e.scope_op).map(|o| o.class);
            match e.kind {
                ToolKind::Read => {
                    assert_eq!(class, Some(OpClass::Read), "{} promises a read", e.name)
                }
                ToolKind::Propose => assert!(
                    class.is_some_and(OpClass::is_external_send),
                    "{} promises a proposal but is not an external send",
                    e.name
                ),
            }
        }
    }

    #[test]
    fn every_proposal_builds_an_l3_action_and_no_read_builds_any() {
        // The whole point of a proposal tool: what it produces is L3, so it cannot auto-run.
        for e in CATALOG {
            let input = json!({
                "to": "a@b.com", "title": "t", "channel": "#c", "target": "x", "body": "b"
            });
            match e.kind {
                ToolKind::Propose => {
                    let action = proposed_action(e, &input)
                        .unwrap_or_else(|| panic!("{} has no action", e.name));
                    assert!(action.is_external_send(), "{} is not a send", e.name);
                    assert_eq!(action.required_level(), Level::L3, "{} is not L3", e.name);
                }
                ToolKind::Read => {
                    assert!(proposed_action(e, &input).is_none(), "{} builds an action", e.name)
                }
            }
        }
    }

    #[test]
    fn a_proposal_without_a_destination_builds_nothing() {
        // An approval prompt needs to say who it goes to. Defaulting the recipient would put an
        // address the user never chose in front of the confirm button.
        let entry = catalog_entry("propose_send_email").unwrap();
        assert!(proposed_action(entry, &json!({ "body": "hi" })).is_none());
        assert!(proposed_action(entry, &json!({ "to": "  ", "body": "hi" })).is_none());
        assert!(proposed_action(entry, &json!({ "to": "a@b.com", "body": "hi" })).is_some());
    }

    #[test]
    fn no_send_is_ever_published_as_a_read() {
        // Invariant 4 at the model's edge: whatever the connection/plan state, the array holds
        // only ops the table classifies as reads. Checked against every service at once.
        let all: Vec<ServiceState> = scope::ALL_SERVICES.iter().copied().map(connected).collect();
        let ctx = ToolContext {
            highest_released: Wave::Three,
            draft_stop: false,
            plan: pro(),
        };
        let tools = tool_definitions(&all, &ctx);
        assert!(!tools.is_empty(), "the fixture must actually produce tools");
        for name in names(&tools) {
            let entry = CATALOG.iter().find(|e| e.name == name).expect("catalog entry");
            let class = scope::lookup(entry.service, entry.scope_op).map(|o| o.class);
            if class.is_some_and(OpClass::is_external_send) {
                assert_eq!(
                    entry.kind,
                    ToolKind::Propose,
                    "{name} is a send published as if it returned data",
                );
            }
        }
    }

    #[test]
    fn every_entry_has_a_schema_of_its_own() {
        // The fallback arm in `input_schema` must never be the one that answers: an empty schema
        // would let the model call a tool with anything at all.
        for e in CATALOG {
            let schema = input_schema(e.name);
            assert_eq!(schema["type"], "object", "{}", e.name);
            assert!(
                schema["properties"].as_object().is_some_and(|p| !p.is_empty()),
                "{} fell through to the empty fallback schema",
                e.name
            );
        }
    }

    #[test]
    fn tool_names_are_unique_and_never_real_mcp_tool_names() {
        let mut seen = std::collections::HashSet::new();
        for e in CATALOG {
            assert!(seen.insert(e.name), "duplicate tool name {}", e.name);
            // The hub name is the stable interface; leaking the server's own name would tie the
            // model's vocabulary to a Developer-Preview server (§5-2).
            assert_ne!(
                Some(e.name),
                shogun_integrations_tool_name(e.service, e.scope_op),
                "{} exposes the real MCP tool name",
                e.name
            );
        }
    }

    /// The real tool names, duplicated here rather than depended on: shogun-mcp must not depend
    /// on shogun-integrations (the dependency runs the other way).
    fn shogun_integrations_tool_name(service: Service, op: &str) -> Option<&'static str> {
        match (service, op) {
            (Service::GoogleCalendar, "read_sync") => Some("list_events"),
            (Service::GoogleCalendar, "free_busy") => Some("suggest_time"),
            (Service::Gmail, "read_sync") => Some("search_threads"),
            (Service::Gmail, "read_on_demand") => Some("get_thread"),
            (Service::GoogleDrive, "read_sync") => Some("list_recent_files"),
            (Service::GoogleDrive, "read_on_demand") => Some("read_file_content"),
            _ => None,
        }
    }

    // ---- generation --------------------------------------------------------------------------

    #[test]
    fn only_connected_services_are_offered() {
        let tools = tool_definitions(&[connected(Service::GoogleCalendar)], &ctx());
        assert_eq!(
            names(&tools),
            vec![
                "list_calendar_events",
                "check_calendar_availability",
                "propose_calendar_event",
                "propose_calendar_event_change",
            ]
        );
    }

    #[test]
    fn a_disconnected_service_contributes_nothing() {
        let tools = tool_definitions(
            &[ServiceState { service: Service::GoogleCalendar, conn: ConnState::Disconnected }],
            &ctx(),
        );
        assert!(tools.is_empty());
        assert_eq!(connected_services_block(&[], &ctx()), None, "an empty list says nothing");
    }

    #[test]
    fn an_unreleased_wave_is_not_offered_even_when_connected() {
        // Wave 1 is out; Slack (Wave 2) is connected in the fixture but must stay invisible.
        let tools = tool_definitions(
            &[connected(Service::GoogleCalendar), connected(Service::Slack)],
            &ctx(),
        );
        assert!(!names(&tools).iter().any(|n| n == "search_chat_messages"));
    }

    #[test]
    fn an_amber_service_still_serves_reads() {
        // FR-INT-06/07: the token is invalid for writes, but cached reads still answer — and the
        // gate is what says so, which is why this module asks it rather than deciding itself.
        let tools = tool_definitions(
            &[ServiceState {
                service: Service::GoogleCalendar,
                conn: ConnState::NeedsReauth { reason: ReauthReason::TokenExpired, last_sync_ms: 0 },
            }],
            &ctx(),
        );
        assert_eq!(names(&tools), vec!["list_calendar_events", "check_calendar_availability"]);
    }

    #[test]
    fn a_plan_without_reads_is_offered_nothing() {
        // An expired trial (issue #97) cannot read, so the model is handed no tools at all rather
        // than tools that will be refused one by one.
        let ctx = ToolContext {
            highest_released: Wave::One,
            draft_stop: true,
            plan: expired_trial(),
        };
        assert!(tool_definitions(&[connected(Service::GoogleCalendar)], &ctx).is_empty());
        assert_eq!(connected_services_block(&[connected(Service::Gmail)], &ctx), None);
    }

    #[test]
    fn unconsented_gmail_is_absent_because_the_caller_treats_it_as_disconnected() {
        // 未同意 = 未接続扱い: the consent decision is folded in before this module sees it.
        let unconsented =
            ServiceState { service: Service::Gmail, conn: ConnState::Disconnected };
        let tools = tool_definitions(&[unconsented, connected(Service::GoogleCalendar)], &ctx());
        assert!(!names(&tools).iter().any(|n| n.contains("mail")));
        let block = connected_services_block(&[unconsented, connected(Service::GoogleCalendar)], &ctx())
            .unwrap();
        assert!(!block.contains("- mail:"), "{block}");
    }

    // ---- the prompt block --------------------------------------------------------------------

    #[test]
    fn the_block_matches_the_canonical_template_for_wave_one() {
        let block = connected_services_block(
            &[
                connected(Service::GoogleCalendar),
                connected(Service::Gmail),
                connected(Service::GoogleDrive),
            ],
            &ctx(),
        )
        .unwrap();
        assert_eq!(
            block,
            "## Connected services\n\
             You can pull context from these connected services:\n\
             - calendar: the user's calendar. Events, availability, upcoming meetings. Read-only.\n\
             - mail: the user's mail. Threads and messages. Read-only; you may draft replies, but sending always requires the user's explicit approval.\n\
             - drive: the user's documents and files. Read-only.\n\
             \n\
             Priorities:\n\
             - Questions about schedule, meetings, or availability → check calendar first.\n\
             - Questions about conversations, requests, or follow-ups → check mail first.\n\
             - Questions about documents or materials → check drive first.\n\
             - For meeting prep, combine: calendar (the event) → drive (related files) → mail (related threads).\n\
             \n\
             If a task would clearly benefit from a service that is not listed here, say so briefly instead of guessing (the user can connect it in Settings).\n"
        );
    }

    #[test]
    fn one_connected_service_gets_no_combination_advice() {
        let block = connected_services_block(&[connected(Service::GoogleCalendar)], &ctx()).unwrap();
        assert!(block.contains("- calendar:"));
        assert!(!block.contains("- mail:"));
        assert!(!block.contains("meeting prep"), "no combination without the services for it");
    }

    #[test]
    fn the_block_never_names_a_tool_or_a_transport() {
        let all: Vec<ServiceState> = scope::ALL_SERVICES.iter().copied().map(connected).collect();
        let ctx = ToolContext {
            highest_released: Wave::Three,
            draft_stop: true,
            plan: pro(),
        };
        let block = connected_services_block(&all, &ctx).unwrap();
        for forbidden in ["Composio", "MCP", "first layer", "second layer", "transport"] {
            assert!(!block.contains(forbidden), "the block leaks {forbidden}: {block}");
        }
        for e in CATALOG {
            assert!(!block.contains(e.name), "the block lists tool name {}", e.name);
        }
    }

    #[test]
    fn the_block_always_carries_the_approval_sentence_when_mail_is_present() {
        let block = connected_services_block(&[connected(Service::Gmail)], &ctx()).unwrap();
        assert!(block.contains("sending always requires the user's explicit approval"));
    }
}
