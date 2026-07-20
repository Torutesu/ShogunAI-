//! Resolve a parsed [`Invocation`] to the Memory API call it will make — the tool, its permission
//! level, and whether a token is present (FR-API-03). This is the CLI's client-side contract with
//! the shared dispatcher; the actual round-trip to the running daemon (REST `127.0.0.1:7464`) is
//! wired when the REST face lands. Until then `shogun` prints this resolution, so the mapping is
//! observable and tested.

use shogun_mcp::memory_api::{tool_level, ApiLevel, Tool};

use crate::command::Command;
use crate::parse::Invocation;

/// A stable, lower-kebab tool name for output (Debug names are not a stable contract).
pub fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::MemorySearch => "memory.search",
        Tool::MemoryGetContext => "memory.get_context",
        Tool::StatePeopleList => "state.people.list",
        Tool::StatePeopleGet => "state.people.get",
        Tool::StateProjectsList => "state.projects.list",
        Tool::StateProjectsGet => "state.projects.get",
        Tool::StateCommitmentsList => "state.commitments.list",
        Tool::StateCommitmentsGet => "state.commitments.get",
        Tool::StateOpenLoopsList => "state.open_loops.list",
        Tool::StateOpenLoopsGet => "state.open_loops.get",
        Tool::MemoryAppendNote => "memory.append_note",
        Tool::StateProposeUpdate => "state.propose_update",
        Tool::ActionsExecute => "actions.execute",
    }
}

/// A short label for a tool's level.
pub fn level_label(level: ApiLevel) -> &'static str {
    match level {
        ApiLevel::Read => "read",
        ApiLevel::Write(shogun_agents::permission::Level::L1) => "L1",
        ApiLevel::Write(shogun_agents::permission::Level::L2) => "L2",
        ApiLevel::Write(shogun_agents::permission::Level::L3) => "L3",
        ApiLevel::PerAction => "per-action",
    }
}

/// Render the resolution of an invocation as a one-line plan (human-readable).
pub fn describe(inv: &Invocation) -> String {
    match inv.command.tool() {
        Some(tool) => {
            let mut line = format!("{} [{}]", tool_name(tool), level_label(tool_level(tool)));
            if inv.token.is_none() {
                // FR-API-03: a call with no token is rejected — even a read.
                line.push_str("  (no token: the daemon will reject this — pass --token or connect the daemon)");
            }
            line
        }
        None => match inv.command {
            Command::ApiStatus => "local: report the running REST port".to_string(),
            Command::Help => "local: help".to_string(),
            // Every other command maps to a tool.
            _ => "local".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::ListOrGet;

    fn inv(command: Command, token: Option<&str>) -> Invocation {
        Invocation { command, token: token.map(str::to_string), json: false, include_low: false }
    }

    #[test]
    fn describes_a_read_with_its_tool_and_level() {
        let line = describe(&inv(Command::People(ListOrGet::List), Some("t")));
        assert!(line.starts_with("state.people.list [read]"), "got: {line}");
        assert!(!line.contains("no token"));
    }

    #[test]
    fn flags_missing_token_even_for_reads() {
        let line = describe(&inv(Command::Search { query: "x".into() }, None));
        assert!(line.contains("memory.search [read]"));
        assert!(line.contains("no token"), "FR-API-03: reads need a token too");
    }

    #[test]
    fn write_levels_are_labelled() {
        assert!(describe(&inv(Command::Note { text: "n".into() }, Some("t"))).contains("memory.append_note [L1]"));
        assert!(describe(&inv(Command::Propose { description: "p".into() }, Some("t")))
            .contains("state.propose_update [L2]"));
        assert!(describe(&inv(Command::Run { agent: "a".into() }, Some("t"))).contains("actions.execute [per-action]"));
    }

    #[test]
    fn local_commands_render_locally() {
        assert_eq!(describe(&inv(Command::ApiStatus, None)), "local: report the running REST port");
        assert_eq!(describe(&inv(Command::Help, None)), "local: help");
    }

    #[test]
    fn tool_names_are_stable_kebab() {
        assert_eq!(tool_name(Tool::StateOpenLoopsGet), "state.open_loops.get");
        assert_eq!(tool_name(Tool::ActionsExecute), "actions.execute");
    }
}
