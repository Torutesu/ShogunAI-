//! The CLI command model and its mapping to Memory API tools (§6.11, FR-API-02). The `shogun`
//! command is symmetric with the UI: every subcommand resolves to the same [`Tool`] the MCP/REST
//! faces call, at the same permission level — so there is no CLI-only capability (invariant 6).

use shogun_mcp::memory_api::Tool;

/// A `list` or `get <id>` on a state noun.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListOrGet {
    List,
    Get { id: i64 },
}

/// A parsed CLI command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `shogun search <query>` → hybrid memory search.
    Search { query: String },
    /// `shogun context [--no-screen]` → the current context cache.
    Context { include_screen: bool },
    /// `shogun people list|get <id>`
    People(ListOrGet),
    /// `shogun projects list|get <id>`
    Projects(ListOrGet),
    /// `shogun commitments list|get <id>`
    Commitments(ListOrGet),
    /// `shogun open-loops list|get <id>`
    OpenLoops(ListOrGet),
    /// `shogun note <text>` → append a user note (L1).
    Note { text: String },
    /// `shogun propose <description>` → propose a state change (L2).
    Propose { description: String },
    /// `shogun run <agent>` → launch a preset agent (level follows the action).
    Run { agent: String },
    /// `shogun actions poll <approval_id>` → durable L3 status.
    ActionsPoll { approval_id: u64 },
    /// `shogun api status` → report the running REST port (FR-API-01).
    ApiStatus,
    /// `shogun metrics` → the in-product SLO snapshot (NFR-SLO-00).
    Metrics,
    /// `shogun visual-recall status|enable|disable|search|frame get|frame rescan`
    VisualRecall(VisualRecallCommand),
    /// `shogun whoami` → profile + work summary.
    Whoami,
    /// `shogun profile set <json>` → update profile prefs (L1).
    ProfileSet { body: String },
    /// `shogun help` / no args.
    Help,
}

/// Visual recall subcommands (Memory API symmetry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualRecallCommand {
    Status,
    Enable,
    Disable,
    SetRetention { days: u8 },
    Search { query: String, from_ms: Option<i64>, to_ms: Option<i64> },
    FrameGet { id: i64 },
    FrameRescan { id: i64 },
    FrameDelete { id: i64 },
}

impl Command {
    /// The Memory API tool this command invokes, if any. `api status` and `help` are local CLI
    /// concerns and map to no tool.
    pub fn tool(&self) -> Option<Tool> {
        Some(match self {
            Command::Search { .. } => Tool::MemorySearch,
            Command::Context { .. } => Tool::MemoryGetContext,
            Command::People(ListOrGet::List) => Tool::StatePeopleList,
            Command::People(ListOrGet::Get { .. }) => Tool::StatePeopleGet,
            Command::Projects(ListOrGet::List) => Tool::StateProjectsList,
            Command::Projects(ListOrGet::Get { .. }) => Tool::StateProjectsGet,
            Command::Commitments(ListOrGet::List) => Tool::StateCommitmentsList,
            Command::Commitments(ListOrGet::Get { .. }) => Tool::StateCommitmentsGet,
            Command::OpenLoops(ListOrGet::List) => Tool::StateOpenLoopsList,
            Command::OpenLoops(ListOrGet::Get { .. }) => Tool::StateOpenLoopsGet,
            Command::Note { .. } => Tool::MemoryAppendNote,
            Command::Propose { .. } => Tool::StateProposeUpdate,
            Command::Run { .. } => Tool::ActionsExecute,
            Command::ActionsPoll { .. } => return None,
            Command::VisualRecall(VisualRecallCommand::Status) => Tool::VisualRecallStatus,
            Command::VisualRecall(VisualRecallCommand::Enable) => Tool::VisualRecallSetEnabled,
            Command::VisualRecall(VisualRecallCommand::Disable) => Tool::VisualRecallSetEnabled,
            Command::VisualRecall(VisualRecallCommand::SetRetention { .. }) => Tool::VisualRecallSetRetention,
            Command::VisualRecall(VisualRecallCommand::Search { .. }) => Tool::VisualRecallSearchFrames,
            Command::VisualRecall(VisualRecallCommand::FrameGet { .. }) => Tool::VisualRecallGetFrame,
            Command::VisualRecall(VisualRecallCommand::FrameRescan { .. }) => Tool::VisualRecallRescanFrame,
            Command::VisualRecall(VisualRecallCommand::FrameDelete { .. }) => Tool::VisualRecallDeleteFrame,
            Command::Whoami => Tool::ProfileWhoami,
            Command::ProfileSet { .. } => Tool::ProfileSet,
            Command::ApiStatus | Command::Metrics | Command::Help => return None,
        })
    }
}

/// The usage text (also printed on a parse error).
pub const USAGE: &str = "\
shogun — SHOGUN Memory API (CLI face)

USAGE:
    shogun [--token <t>] [--json] <command>

COMMANDS:
    search <query>            Hybrid memory search
    context [--no-screen]     Current context cache
    people list|get <id>      People state
    projects list|get <id>    Projects state
    commitments list|get <id> Commitments state
    open-loops list|get <id>  Open loops state
    note <text>               Append a user note            (L1)
    propose <description>     Propose a state change        (L2)
    run <agent>               Launch a preset agent         (level follows action)
    actions poll <id>         Poll L3 approval status
    api status                Show the running REST port
    metrics                   In-product SLO snapshot
    visual-recall status      Visual recall status + frame stats
    visual-recall enable      Turn visual recall on (L1)
    visual-recall disable     Turn passive recall off + purge auto frames (L1)
    visual-recall retention <1|3|5|7>      Set saved-screen lifetime (L1)
    visual-recall search <q> [--from-ms N] [--to-ms N]  Search stored screen frames
    visual-recall frame get <id>           Frame metadata + OCR text
    visual-recall frame rescan <id>        Re-OCR stored JPEG
    visual-recall frame delete <id>        Delete one stored frame
    whoami                    Profile + short work summary
    profile set <json>        Update profile prefs          (L1)
    help                      This help

GLOBAL FLAGS:
    --token <t>   API token (else read from Keychain when the daemon is running)
    --json        Machine-readable output
    --include-low Include <0.5 confidence results (reads; default excludes them)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_commands_map_to_read_tools() {
        use shogun_mcp::memory_api::{tool_level, ApiLevel};
        for (cmd, _) in [
            (Command::Search { query: "x".into() }, ()),
            (Command::Context { include_screen: true }, ()),
            (Command::People(ListOrGet::List), ()),
            (Command::Commitments(ListOrGet::Get { id: 1 }), ()),
        ] {
            let tool = cmd.tool().unwrap();
            assert_eq!(tool_level(tool), ApiLevel::Read, "{cmd:?} must be a read");
        }
    }

    #[test]
    fn note_is_l1_propose_is_l2_run_is_per_action() {
        use shogun_agents::permission::Level;
        use shogun_mcp::memory_api::{tool_level, ApiLevel};
        assert_eq!(tool_level(Command::Note { text: "n".into() }.tool().unwrap()), ApiLevel::Write(Level::L1));
        assert_eq!(
            tool_level(Command::Propose { description: "p".into() }.tool().unwrap()),
            ApiLevel::Write(Level::L2)
        );
        assert_eq!(tool_level(Command::Run { agent: "a".into() }.tool().unwrap()), ApiLevel::PerAction);
    }

    #[test]
    fn local_commands_have_no_tool() {
        assert!(Command::ApiStatus.tool().is_none());
        assert!(Command::Help.tool().is_none());
    }

    #[test]
    fn get_variants_map_to_get_tools() {
        assert_eq!(Command::People(ListOrGet::Get { id: 3 }).tool(), Some(Tool::StatePeopleGet));
        assert_eq!(Command::Projects(ListOrGet::List).tool(), Some(Tool::StateProjectsList));
    }
}
