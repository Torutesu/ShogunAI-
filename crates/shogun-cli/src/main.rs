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
