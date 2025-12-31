//! sv - Simultaneous Versioning CLI
//!
//! A standalone CLI that makes Git practical for many parallel agents by adding
//! workspaces, leases, protected paths, risk prediction, and operation undo.

use clap::Parser;
use sv::cli::Cli;
use sv::output::{emit_error, infer_command_name_from_args};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let command = infer_command_name_from_args();
    let cli = Cli::parse();
    let events_to_stdout = cli
        .events
        .as_deref()
        .map(|value| value.trim() == "-")
        .unwrap_or(false);
    let json = cli.json && !events_to_stdout;
    if let Err(err) = cli.run() {
        let _ = emit_error(&command, &err, json);
        std::process::exit(err.exit_code());
    }
}
