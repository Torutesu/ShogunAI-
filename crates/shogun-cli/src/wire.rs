//! Map a parsed [`Command`] to the HTTP call the CLI makes against the local Memory API server
//! (§6.11). Pure — the actual socket work is [`crate::http`]. This is the CLI's half of the REST
//! contract; the server's half is `shogun_mcp::rest`.

use crate::command::{Command, ListOrGet, VisualRecallCommand};

/// An HTTP call: method + path (query folded in) + optional body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCall {
    pub method: &'static str,
    pub path: String,
    pub body: Option<String>,
}

/// Minimal percent-encoding for a query value (space and the reserved delimiters that would break
/// the query string). Enough for a search term.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn state_path(noun: &str, which: &ListOrGet) -> String {
    match which {
        ListOrGet::List => format!("/v1/state/{noun}"),
        ListOrGet::Get { id } => format!("/v1/state/{noun}/{id}"),
    }
}

/// The HTTP call for a command, or `None` for a purely local command (`help`). `include_low` adds
/// the read opt-in query.
pub fn to_call(command: &Command, include_low: bool) -> Option<HttpCall> {
    let get = |path: String| HttpCall { method: "GET", path, body: None };
    let post = |path: String, body: String| HttpCall { method: "POST", path, body: Some(body) };
    let low = |mut path: String| {
        if include_low {
            path.push_str(if path.contains('?') { "&include_low" } else { "?include_low" });
        }
        path
    };

    Some(match command {
        Command::Search { query } => get(low(format!("/v1/memory/search?q={}", encode(query)))),
        Command::Context { .. } => get(low("/v1/memory/context".to_string())),
        Command::People(w) => get(low(state_path("people", w))),
        Command::Projects(w) => get(low(state_path("projects", w))),
        Command::Commitments(w) => get(low(state_path("commitments", w))),
        Command::OpenLoops(w) => get(low(state_path("open_loops", w))),
        Command::Note { text } => post("/v1/memory/notes".into(), text.clone()),
        Command::Propose { description } => post("/v1/state/proposals".into(), description.clone()),
        // `run` carries the action JSON spec (e.g. '{"kind":"local_search","query":"x"}').
        Command::Run { agent } => post("/v1/actions/execute".into(), agent.clone()),
        Command::Onboarding => get("/v1/device/onboarding".to_string()),
        Command::ApiStatus => get("/v1/status".to_string()),
        Command::Metrics => get("/v1/metrics".to_string()),
        Command::VisualRecall(cmd) => match cmd {
            VisualRecallCommand::Status => get("/v1/visual_recall/status".to_string()),
            VisualRecallCommand::Enable => post("/v1/visual_recall/enabled".into(), r#"{"enabled":true}"#.to_string()),
            VisualRecallCommand::Disable => post("/v1/visual_recall/enabled".into(), r#"{"enabled":false}"#.to_string()),
            VisualRecallCommand::Search { query, from_ms, to_ms } => {
                let mut path = format!("/v1/visual_recall/frames/search?q={}", encode(query));
                if let Some(f) = from_ms {
                    path.push_str(&format!("&from_ms={f}"));
                }
                if let Some(t) = to_ms {
                    path.push_str(&format!("&to_ms={t}"));
                }
                get(low(path))
            }
            VisualRecallCommand::FrameGet { id } => get(format!("/v1/visual_recall/frames/{id}")),
            VisualRecallCommand::FrameRescan { id } => {
                post(format!("/v1/visual_recall/frames/{id}/rescan"), String::new())
            }
            VisualRecallCommand::FrameDelete { id } => {
                post("/v1/visual_recall/frames/delete".into(), format!(r#"{{"id":{id}}}"#))
            }
        },
        Command::Help | Command::Config { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_gets_with_the_right_paths() {
        assert_eq!(
            to_call(&Command::People(ListOrGet::List), false).unwrap(),
            HttpCall { method: "GET", path: "/v1/state/people".into(), body: None }
        );
        assert_eq!(
            to_call(&Command::Commitments(ListOrGet::Get { id: 7 }), false).unwrap().path,
            "/v1/state/commitments/7"
        );
        assert_eq!(to_call(&Command::OpenLoops(ListOrGet::List), false).unwrap().path, "/v1/state/open_loops");
    }

    #[test]
    fn search_encodes_the_query() {
        let call = to_call(&Command::Search { query: "budget review".into() }, false).unwrap();
        assert_eq!(call.path, "/v1/memory/search?q=budget%20review");
    }

    #[test]
    fn include_low_is_appended_correctly() {
        // no existing query → ?include_low
        assert_eq!(to_call(&Command::People(ListOrGet::List), true).unwrap().path, "/v1/state/people?include_low");
        // existing query → &include_low
        let s = to_call(&Command::Search { query: "x".into() }, true).unwrap().path;
        assert_eq!(s, "/v1/memory/search?q=x&include_low");
    }

    #[test]
    fn writes_and_actions_are_posts_with_bodies() {
        assert_eq!(
            to_call(&Command::Note { text: "call Bob".into() }, false).unwrap(),
            HttpCall { method: "POST", path: "/v1/memory/notes".into(), body: Some("call Bob".into()) }
        );
        assert_eq!(
            to_call(&Command::Propose { description: "add a commitment".into() }, false).unwrap().path,
            "/v1/state/proposals"
        );
        let run = to_call(&Command::Run { agent: r#"{"kind":"local_search","query":"x"}"#.into() }, false).unwrap();
        assert_eq!(run.method, "POST");
        assert_eq!(run.path, "/v1/actions/execute");
        assert_eq!(run.body.as_deref(), Some(r#"{"kind":"local_search","query":"x"}"#));
    }

    #[test]
    fn api_status_is_a_get_and_help_has_no_call() {
        assert_eq!(to_call(&Command::ApiStatus, false).unwrap().path, "/v1/status");
        assert!(to_call(&Command::Help, false).is_none());
    }
}
