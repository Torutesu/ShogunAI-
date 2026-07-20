//! The `shogun` binary — a thin runner over the pure CLI logic in the `shogun_cli` library.
//!
//! It parses argv, and either prints help, prints a parse error (exit 2), or resolves the command
//! to its Memory API call and prints that. Actually dispatching the call round-trips to the running
//! daemon's REST endpoint (`127.0.0.1:7464`); that transport is wired when the REST face lands, so
//! today the binary reports the resolution rather than executing it.

use std::process::ExitCode;

use shogun_cli::command::Command;
use shogun_cli::parse::parse;
use shogun_cli::{command, plan};

fn main() -> ExitCode {
    // Skip the program name.
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

    // Until the REST face is wired, report the resolved call rather than executing it.
    println!("{}", plan::describe(&invocation));
    ExitCode::SUCCESS
}
