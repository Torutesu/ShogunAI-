//! The `shogun` binary — parses argv and calls the local Memory API server over loopback.
//!
//! Config via env: `SHOGUN_API_PORT` (default 7464), `SHOGUN_API_TOKEN` (used when `--token` is not
//! given). The token is sent as a `Bearer` header; without one, tool calls get 401 (as the daemon
//! requires, FR-API-03). Exit code mirrors the outcome: 0 success, 1 an HTTP error status, 2 a
//! parse error, 3 the daemon is unreachable.

use std::process::ExitCode;

use shogun_cli::command::{self, Command};
use shogun_cli::parse::parse;
use shogun_cli::{http, wire};

const DEFAULT_PORT: u16 = 7464;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let invocation = match parse(&args) {
        Ok(inv) => inv,
        Err(err) => {
            eprintln!("error: {}\n\n{}", err.message(), command::USAGE);
            return ExitCode::from(2);
        }
    };

    if invocation.command == Command::Help {
        println!("{}", command::USAGE);
        return ExitCode::SUCCESS;
    }

    if let command::Command::Config { action } = &invocation.command {
        use shogun_core::user_config::{default_path, load_report};
        let Some(path) = default_path() else {
            eprintln!("error: could not resolve home dir");
            return ExitCode::from(1);
        };
        match action {
            command::ConfigAction::Path => {
                println!("{}", path.display());
            }
            command::ConfigAction::Show => match load_report(&path) {
                Ok((cfg, _)) => println!("{cfg:#?}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            },
            command::ConfigAction::Validate => match load_report(&path) {
                Ok((_, report)) => {
                    if report.ok {
                        println!("ok");
                    } else {
                        for e in &report.section_errors {
                            println!("{}:{} {}", e.section, e.line, e.message);
                        }
                        return ExitCode::from(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            },
        }
        return ExitCode::SUCCESS;
    }

    let Some(call) = wire::to_call(&invocation.command, invocation.include_low) else {
        println!("{}", command::USAGE);
        return ExitCode::SUCCESS;
    };

    let port = std::env::var("SHOGUN_API_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let token = invocation.token.or_else(|| std::env::var("SHOGUN_API_TOKEN").ok().filter(|t| !t.is_empty()));

    match http::request(port, call.method, &call.path, token.as_deref(), call.body.as_deref()) {
        Ok(resp) => {
            println!("{}", resp.body);
            if resp.status < 400 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("could not reach the daemon on 127.0.0.1:{port}: {e}");
            eprintln!("(is `shogun-api` running?  start it with: cargo run -p shogun-core --features daemon-server --bin shogun-api)");
            ExitCode::from(3)
        }
    }
}
