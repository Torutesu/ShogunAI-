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

/// The action for `shogun config path|show|validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigAction {
    Path,
    Show,
    Validate,
}

/// A parsed CLI command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `shogun search <query>` → hybrid memory search.
    Search { query: String },
    /// `shogun context [--no-screen]` → the current context cache.
    Context { include_screen: bool },
    /// `shogun pack <query>` → the grounded context pack for a task/question (FR-API-08).
    Pack { query: String },
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
    /// `shogun actions poll <approval_id>` returns a durable, body-free L3 outcome.
    ActionsPoll { approval_id: u64 },
    /// `shogun wrap` → today's Evening Wrap (issue #10, invariant 6 — the notch card as a read).
    Wrap,
    /// `shogun onboarding` → this device's onboarding / first-run setup state (issue #6).
    Onboarding,
    /// `shogun api status` → report the running REST port (FR-API-01).
    ApiStatus,
    /// `shogun metrics` → the in-product SLO snapshot (NFR-SLO-00).
    Metrics,
    /// `shogun lessons list|enable <id>|disable <id>` (L5, Plan D-5 — Memory API symmetry).
    Lessons(LessonsCommand),
    /// `shogun visual-recall status|enable|disable|search|frame get|frame rescan`
    VisualRecall(VisualRecallCommand),
    /// `shogun whoami` → profile and compact work summary.
    Whoami,
    /// `shogun profile set <json>` → local L1 profile update.
    ProfileSet { body: String },
    /// `shogun help` / no args.
    Help,
    /// `shogun config path|show|validate`
    Config { action: ConfigAction },
}

/// Lessons subcommands (invariant 6: same list + toggle the Learned UI offers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LessonsCommand {
    List,
    Enable { id: i64 },
    Disable { id: i64 },
}

/// Visual recall subcommands (Memory API symmetry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualRecallCommand {
    Status,
    Enable,
    Disable,
    Search {
        query: String,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    },
    FrameGet {
        id: i64,
    },
    FrameRescan {
        id: i64,
    },
    FrameDelete {
        id: i64,
    },
}

impl Command {
    /// The Memory API tool this command invokes, if any. `api status` and `help` are local CLI
    /// concerns and map to no tool.
    pub fn tool(&self) -> Option<Tool> {
        Some(match self {
            Command::Search { .. } => Tool::MemorySearch,
            Command::Context { .. } => Tool::MemoryGetContext,
            Command::Pack { .. } => Tool::MemoryGetContextPack,
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
            Command::Wrap => Tool::MemoryGetWrap,
            Command::Onboarding => Tool::DeviceOnboardingGet,
            Command::Lessons(LessonsCommand::List) => Tool::LessonsList,
            Command::Lessons(LessonsCommand::Enable { .. })
            | Command::Lessons(LessonsCommand::Disable { .. }) => Tool::LessonsSetActive,
            Command::VisualRecall(VisualRecallCommand::Status) => Tool::VisualRecallStatus,
            Command::VisualRecall(VisualRecallCommand::Enable) => Tool::VisualRecallSetEnabled,
            Command::VisualRecall(VisualRecallCommand::Disable) => Tool::VisualRecallSetEnabled,
            Command::VisualRecall(VisualRecallCommand::Search { .. }) => {
                Tool::VisualRecallSearchFrames
            }
            Command::VisualRecall(VisualRecallCommand::FrameGet { .. }) => {
                Tool::VisualRecallGetFrame
            }
            Command::VisualRecall(VisualRecallCommand::FrameRescan { .. }) => {
                Tool::VisualRecallRescanFrame
            }
            Command::VisualRecall(VisualRecallCommand::FrameDelete { .. }) => {
                Tool::VisualRecallDeleteFrame
            }
            Command::Whoami => Tool::ProfileWhoami,
            Command::ProfileSet { .. } => Tool::ProfileSet,
            Command::ApiStatus | Command::Metrics | Command::Help | Command::Config { .. } => {
                return None
            }
        })
    }
}

/// The usage text (also printed on a parse error).
pub const USAGE: &str = "\
shogun — SHOGUN Memory API (CLI face)

USAGE:
    shogun [--token <t>] <command>

COMMANDS:
    search <query>            Hybrid memory search
    context                   Current context cache
    pack <query>              Grounded context pack (facts + evidence with provenance)
    people list|get <id>      People state
    projects list|get <id>    Projects state
    commitments list|get <id> Commitments state
    open-loops list|get <id>  Open loops state
    note <text>               Append a user note            (L1)
    propose <description>     Propose a state change        (L2)
    run <agent>               Launch a preset agent         (level follows action)
    actions poll <id>         Poll L3 approval status
    wrap                      Today's Evening Wrap (outcome, still open, tomorrow)
    onboarding                This device's first-run setup state
    api status                Show the running REST port
    metrics                   In-product SLO snapshot + lesson counters
    lessons list              Learned lessons (instruction, confidence, active)
    lessons enable <id>       Switch a learned lesson on  (L1)
    lessons disable <id>      Switch a learned lesson off (L1)
    visual-recall status      Visual recall status + frame stats
    visual-recall enable      Turn visual recall on (L1)
    visual-recall disable     Turn passive recall off + purge auto frames (L1)
    visual-recall search <q> [--from-ms N] [--to-ms N]  Search stored screen frames
    visual-recall frame get <id>           Frame metadata + OCR text
    visual-recall frame rescan <id>        Re-OCR stored JPEG
    visual-recall frame delete <id>        Delete one stored frame
    whoami                    Profile + compact work summary
    profile set <json>        Update profile preferences   (L1)
    config path|show|validate Show the Shougun.md path, parsed config, or validation
    help                      This help

GLOBAL FLAGS:
    --token <t>   API token (else the SHOGUN_API_TOKEN environment variable)
    --include-low Include <0.5 confidence results (reads; default excludes them)

Output is the server's JSON, as received.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_commands_map_to_read_tools() {
        use shogun_mcp::memory_api::{tool_level, ApiLevel};
        for (cmd, _) in [
            (Command::Search { query: "x".into() }, ()),
            (
                Command::Context {
                    include_screen: true,
                },
                (),
            ),
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
        assert_eq!(
            tool_level(Command::Note { text: "n".into() }.tool().unwrap()),
            ApiLevel::Write(Level::L1)
        );
        assert_eq!(
            tool_level(
                Command::Propose {
                    description: "p".into()
                }
                .tool()
                .unwrap()
            ),
            ApiLevel::Write(Level::L2)
        );
        assert_eq!(
            tool_level(Command::Run { agent: "a".into() }.tool().unwrap()),
            ApiLevel::PerAction
        );
    }

    #[test]
    fn local_commands_have_no_tool() {
        assert!(Command::ApiStatus.tool().is_none());
        assert!(Command::Help.tool().is_none());
        assert!(Command::Config {
            action: ConfigAction::Path
        }
        .tool()
        .is_none());
    }

    #[test]
    fn lessons_commands_map_to_the_lessons_tools_at_ui_levels() {
        use shogun_agents::permission::Level;
        use shogun_mcp::memory_api::{tool_level, ApiLevel};
        assert_eq!(
            Command::Lessons(LessonsCommand::List).tool(),
            Some(Tool::LessonsList)
        );
        assert_eq!(tool_level(Tool::LessonsList), ApiLevel::Read);
        for cmd in [
            Command::Lessons(LessonsCommand::Enable { id: 1 }),
            Command::Lessons(LessonsCommand::Disable { id: 1 }),
        ] {
            assert_eq!(cmd.tool(), Some(Tool::LessonsSetActive));
            assert_eq!(tool_level(cmd.tool().unwrap()), ApiLevel::Write(Level::L1));
        }
    }

    #[test]
    fn get_variants_map_to_get_tools() {
        assert_eq!(
            Command::People(ListOrGet::Get { id: 3 }).tool(),
            Some(Tool::StatePeopleGet)
        );
        assert_eq!(
            Command::Projects(ListOrGet::List).tool(),
            Some(Tool::StateProjectsList)
        );
    }
}
