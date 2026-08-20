use shogun_agents::approval::{ApprovalOrigin, ApprovalQueue, Preview, Route};
use shogun_agents::permission::{Action, LocalAction, SendAction};

use super::{json_escape, level_label};

enum ActionSpec {
    Local(LocalAction),
    Send(SendAction, Preview),
}

/// Parse the `actions.execute` JSON body into an action. Only string-parameterised actions are
/// expressible over the API (`SaveDraft`/`UpdateState` carry `'static` targets and are launched
/// from the UI, not the wire). Unknown / malformed bodies return `None` (→ 400).
fn parse_action(body: &str) -> Option<ActionSpec> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let kind = v.get("kind")?.as_str()?;
    let field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);

    // For a send, the L3 preview shows the full content (FR-AG-03). Gmail send routes via Composio.
    let send = |action: SendAction, full: String, route: Route| {
        let preview = Preview::for_send(&action, full, route);
        ActionSpec::Send(action, preview)
    };

    Some(match kind {
        "local_search" => ActionSpec::Local(LocalAction::LocalSearch {
            query: field("query")?,
        }),
        "open_app" => ActionSpec::Local(LocalAction::OpenApp {
            bundle_id: field("bundle_id")?,
        }),
        "reveal_file" => ActionSpec::Local(LocalAction::RevealFile {
            path: field("path")?,
        }),
        "show_notification" => ActionSpec::Local(LocalAction::ShowNotification {
            text: field("text")?,
        }),
        "copy_to_clipboard" => ActionSpec::Local(LocalAction::CopyToClipboard {
            text: field("text")?,
        }),
        "send_email" => {
            let to = field("to")?;
            let full = format!(
                "Subject: {}\n\n{}",
                field("subject").unwrap_or_default(),
                field("body").unwrap_or_default()
            );
            send(SendAction::SendEmail { to }, full, Route::ViaComposio)
        }
        "post_message" => {
            let channel = field("channel")?;
            send(
                SendAction::PostMessage { channel },
                field("body").unwrap_or_default(),
                Route::DirectMcp,
            )
        }
        "create_calendar_event" => {
            let title = field("title")?;
            send(
                SendAction::CreateCalendarEvent {
                    title: title.clone(),
                },
                title,
                Route::DirectMcp,
            )
        }
        "post_comment" => {
            let target = field("target")?;
            send(
                SendAction::PostComment { target },
                field("body").unwrap_or_default(),
                Route::DirectMcp,
            )
        }
        _ => return None,
    })
}

/// Whether this process has a surface that will ever drain the approval queue.
///
/// The desktop app has one (the Notch confirm UI); the standalone `shogun-api` / `shogun-mcp`
/// binaries do not — they build a queue at their composition root that nobody watches. Enqueuing
/// an L3 send there is not dangerous (invariant 4 still holds: nothing sends without a human),
/// but it *looks* like it worked and then expires in silence, which quietly breaks invariant 6's
/// promise that the API face behaves like the human one. Saying so is the honest outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalSurface {
    /// A confirm UI is running in this process.
    Present,
    /// Headless: refuse external sends rather than strand them.
    Absent,
}

/// Handle `actions.execute` (auth already enforced by [`route`]). A local action is authorized to
/// run (200); an external send is enqueued in the shared approval queue and returns pending +
/// approval id (202, FR-API-04) — it never runs here without a UI confirm. `origin` labels which
/// face enqueued it (REST/CLI = Api, stdio = Mcp) in the single shared queue (B-3 / E-08).
/// `surface` says whether anything in this process can confirm; see [`ApprovalSurface`].
pub fn act(
    body: Option<&str>,
    now_ms: i64,
    approvals: &mut ApprovalQueue,
    origin: ApprovalOrigin,
    surface: ApprovalSurface,
) -> (u16, String) {
    let Some(body) = body else {
        return (400, r#"{"error":"missing_body"}"#.to_string());
    };
    match parse_action(body) {
        None => (400, r#"{"error":"bad_action_request"}"#.to_string()),
        Some(ActionSpec::Local(action)) => {
            let level = Action::Local(action).required_level();
            (
                200,
                format!(r#"{{"executed":"local","level":"{}"}}"#, level_label(level)),
            )
        }
        Some(ActionSpec::Send(send, preview)) => {
            if surface == ApprovalSurface::Absent {
                return (
                    501,
                    r#"{"error":"no_approval_surface","detail":"external sends need the SHOGUN app running to confirm them"}"#
                        .to_string(),
                );
            }
            // Refuse rows the persisted store would reject on load (empty/oversized destination,
            // oversized body, MAX_PENDING) — otherwise one bad enqueue writes a file every later
            // `load_queue` refuses, bricking the shared queue for every face.
            match crate::approval_store::validate_enqueue(approvals, &preview) {
                Err(crate::approval_store::EnqueueRefusal::Invalid(detail)) => {
                    return (
                        400,
                        format!(
                            r#"{{"error":"bad_action_request","detail":"{}"}}"#,
                            json_escape(detail)
                        ),
                    );
                }
                Err(crate::approval_store::EnqueueRefusal::QueueFull) => {
                    return (429, r#"{"error":"approval_queue_full"}"#.to_string());
                }
                Ok(()) => {}
            }
            let now = u64::try_from(now_ms).unwrap_or(0);
            match approvals.try_request(send, preview, origin, now) {
                Ok(id) => (
                    202,
                    format!(
                        r#"{{"pending":true,"approval_id":{},"level":"L3","origin":"{}"}}"#,
                        id.0,
                        origin.as_str()
                    ),
                ),
                // Id exhaustion: refuse rather than panic while the shared queue lock is held.
                Err(_) => (503, r#"{"error":"approval_queue_unavailable"}"#.to_string()),
            }
        }
    }
}
