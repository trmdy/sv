//! sv - Simultaneous Versioning CLI
//!
//! A standalone CLI that makes Git practical for many parallel agents by adding
//! workspaces, leases, protected paths, risk prediction, and operation undo.

use clap::Parser;
use sv::cli::Cli;
use sv::error::JsonError;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let json = cli.json;
    if let Err(err) = cli.run() {
        if json {
            let payload = JsonError::from(&err);
            let text = serde_json::to_string(&payload).unwrap_or_else(|_| {
                format!(r#"{{"error":"{}","code":{}}}"#, err, err.exit_code())
            });
            println!("{text}");
        } else {
            eprintln!("error: {err}");
        }
        std::process::exit(err.exit_code());
    }
}
