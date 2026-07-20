//! The `shogun` argument parser — a pure `parse(&[String]) -> Result<Invocation, CliError>`, so the
//! whole command grammar is unit-testable without spawning a process. Dependency-free (no clap):
//! the grammar is small and the tests are the spec.

use crate::command::{Command, ListOrGet};

/// A fully parsed invocation: the command plus global flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub command: Command,
    /// `--token <t>` (else the daemon reads it from the Keychain).
    pub token: Option<String>,
    /// `--json` machine-readable output.
    pub json: bool,
    /// `--include-low` — include <0.5 confidence read results (FR-API-06 opt-in).
    pub include_low: bool,
}

/// A parse failure, with a message suitable for stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// A flag that needs a value was given none (e.g. trailing `--token`).
    MissingFlagValue(&'static str),
    /// A subcommand needs an argument it didn't get.
    MissingArgument(&'static str),
    /// An id argument wasn't an integer.
    BadId(String),
    /// The subcommand isn't recognised.
    UnknownCommand(String),
    /// A recognised subcommand got a sub-argument it doesn't understand.
    UnknownSubcommand { command: &'static str, got: String },
}

impl CliError {
    pub fn message(&self) -> String {
        match self {
            CliError::MissingFlagValue(f) => format!("flag `{f}` needs a value"),
            CliError::MissingArgument(a) => format!("missing argument: {a}"),
            CliError::BadId(s) => format!("invalid id: `{s}` (expected an integer)"),
            CliError::UnknownCommand(c) => format!("unknown command: `{c}` (try `shogun help`)"),
            CliError::UnknownSubcommand { command, got } => {
                format!("unknown `{command}` subcommand: `{got}` (expected list|get)")
            }
        }
    }
}

/// Parse argv (excluding the program name).
pub fn parse(args: &[String]) -> Result<Invocation, CliError> {
    let mut token = None;
    let mut json = false;
    let mut include_low = false;
    let mut no_screen = false;
    let mut positionals: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--token" => {
                let v = args.get(i + 1).ok_or(CliError::MissingFlagValue("--token"))?;
                token = Some(v.clone());
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            "--include-low" => {
                include_low = true;
                i += 1;
            }
            "--no-screen" => {
                no_screen = true;
                i += 1;
            }
            _ => {
                positionals.push(args[i].clone());
                i += 1;
            }
        }
    }

    let command = parse_command(&positionals, no_screen)?;
    Ok(Invocation { command, token, json, include_low })
}

/// A `list` / `get <id>` sub-grammar shared by the state nouns.
fn parse_list_or_get(command: &'static str, rest: &[String]) -> Result<ListOrGet, CliError> {
    match rest.first().map(String::as_str) {
        None | Some("list") => Ok(ListOrGet::List),
        Some("get") => {
            let id_str = rest.get(1).ok_or(CliError::MissingArgument("<id>"))?;
            let id = id_str.parse::<i64>().map_err(|_| CliError::BadId(id_str.clone()))?;
            Ok(ListOrGet::Get { id })
        }
        Some(other) => Err(CliError::UnknownSubcommand { command, got: other.to_string() }),
    }
}

fn parse_command(positionals: &[String], no_screen: bool) -> Result<Command, CliError> {
    let Some(head) = positionals.first().map(String::as_str) else {
        return Ok(Command::Help);
    };
    let rest = &positionals[1..];
    match head {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "search" => {
            let query = join(rest).ok_or(CliError::MissingArgument("<query>"))?;
            Ok(Command::Search { query })
        }
        "context" => Ok(Command::Context { include_screen: !no_screen }),
        "people" => Ok(Command::People(parse_list_or_get("people", rest)?)),
        "projects" => Ok(Command::Projects(parse_list_or_get("projects", rest)?)),
        "commitments" => Ok(Command::Commitments(parse_list_or_get("commitments", rest)?)),
        "open-loops" => Ok(Command::OpenLoops(parse_list_or_get("open-loops", rest)?)),
        "note" => {
            let text = join(rest).ok_or(CliError::MissingArgument("<text>"))?;
            Ok(Command::Note { text })
        }
        "propose" => {
            let description = join(rest).ok_or(CliError::MissingArgument("<description>"))?;
            Ok(Command::Propose { description })
        }
        "run" => {
            // The remaining args form the action JSON spec (e.g. '{"kind":"local_search",...}').
            let agent = join(rest).ok_or(CliError::MissingArgument("<action-json>"))?;
            Ok(Command::Run { agent })
        }
        "api" => match rest.first().map(String::as_str) {
            Some("status") => Ok(Command::ApiStatus),
            other => Err(CliError::UnknownSubcommand {
                command: "api",
                got: other.unwrap_or("").to_string(),
            }),
        },
        other => Err(CliError::UnknownCommand(other.to_string())),
    }
}

/// Join trailing words into one string (for free-text args); `None` if empty.
fn join(words: &[String]) -> Option<String> {
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_is_help() {
        assert_eq!(parse(&[]).unwrap().command, Command::Help);
    }

    #[test]
    fn search_joins_multi_word_query() {
        let inv = parse(&v(&["search", "budget", "review"])).unwrap();
        assert_eq!(inv.command, Command::Search { query: "budget review".into() });
    }

    #[test]
    fn search_without_query_errors() {
        assert_eq!(parse(&v(&["search"])), Err(CliError::MissingArgument("<query>")));
    }

    #[test]
    fn global_flags_are_extracted_anywhere() {
        let inv = parse(&v(&["--token", "abc", "search", "x", "--json"])).unwrap();
        assert_eq!(inv.token.as_deref(), Some("abc"));
        assert!(inv.json);
        assert_eq!(inv.command, Command::Search { query: "x".into() });
    }

    #[test]
    fn token_without_value_errors() {
        assert_eq!(parse(&v(&["search", "x", "--token"])), Err(CliError::MissingFlagValue("--token")));
    }

    #[test]
    fn context_no_screen_flag() {
        assert_eq!(parse(&v(&["context"])).unwrap().command, Command::Context { include_screen: true });
        assert_eq!(
            parse(&v(&["context", "--no-screen"])).unwrap().command,
            Command::Context { include_screen: false }
        );
    }

    #[test]
    fn state_noun_list_and_get() {
        assert_eq!(parse(&v(&["people"])).unwrap().command, Command::People(ListOrGet::List));
        assert_eq!(parse(&v(&["people", "list"])).unwrap().command, Command::People(ListOrGet::List));
        assert_eq!(
            parse(&v(&["people", "get", "42"])).unwrap().command,
            Command::People(ListOrGet::Get { id: 42 })
        );
    }

    #[test]
    fn get_without_id_errors_and_bad_id_errors() {
        assert_eq!(parse(&v(&["projects", "get"])), Err(CliError::MissingArgument("<id>")));
        assert_eq!(parse(&v(&["projects", "get", "x"])), Err(CliError::BadId("x".into())));
    }

    #[test]
    fn unknown_subcommand_of_state_noun() {
        assert_eq!(
            parse(&v(&["commitments", "delete"])),
            Err(CliError::UnknownSubcommand { command: "commitments", got: "delete".into() })
        );
    }

    #[test]
    fn note_and_propose_join_text() {
        assert_eq!(
            parse(&v(&["note", "call", "Alice", "tomorrow"])).unwrap().command,
            Command::Note { text: "call Alice tomorrow".into() }
        );
        assert_eq!(
            parse(&v(&["propose", "add", "commitment"])).unwrap().command,
            Command::Propose { description: "add commitment".into() }
        );
    }

    #[test]
    fn include_low_flag_sets_the_field() {
        let inv = parse(&v(&["people", "list", "--include-low"])).unwrap();
        assert!(inv.include_low);
    }

    #[test]
    fn api_status_and_unknown() {
        assert_eq!(parse(&v(&["api", "status"])).unwrap().command, Command::ApiStatus);
        assert!(matches!(parse(&v(&["api", "whoami"])), Err(CliError::UnknownSubcommand { .. })));
        assert_eq!(parse(&v(&["frobnicate"])), Err(CliError::UnknownCommand("frobnicate".into())));
    }

    #[test]
    fn run_takes_the_action_json() {
        assert_eq!(
            parse(&v(&["run", r#"{"kind":"local_search","query":"x"}"#])).unwrap().command,
            Command::Run { agent: r#"{"kind":"local_search","query":"x"}"#.into() }
        );
        assert_eq!(parse(&v(&["run"])), Err(CliError::MissingArgument("<action-json>")));
    }
}
