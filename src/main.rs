//! sv - Simultaneous Versioning CLI
//!
//! A standalone CLI that makes Git practical for many parallel agents by adding
//! workspaces, leases, protected paths, risk prediction, and operation undo.

use clap::Parser;
use sv::cli::Cli;
use sv::output::{emit_error, infer_command_name_from_args};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() {
    // Windows executables default to a much smaller main thread stack than Unix.
    // Some clap parsing / command construction paths can overflow it. Run the
    // CLI on a dedicated thread with a larger stack on Windows.
    #[cfg(windows)]
    {
        let stack_size = 8 * 1024 * 1024;
        let handle = std::thread::Builder::new()
            .name("sv-main".to_string())
            .stack_size(stack_size)
            .spawn(real_main)
            .expect("spawn sv main thread");
        let exit_code = handle.join().unwrap_or(1);
        std::process::exit(exit_code);
    }

    #[cfg(not(windows))]
    {
        std::process::exit(real_main());
    }
}

fn real_main() -> i32 {
    // Tracing is opt-in via RUST_LOG.
    // Keep startup robust in CI/robot envs: ignore invalid/huge filters.
    if let Some(filter) = std::env::var("RUST_LOG").ok().and_then(|raw| {
        let raw = raw.trim();
        if raw.is_empty() || raw.len() > 4096 {
            return None;
        }
        EnvFilter::try_new(raw).ok()
    }) {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(filter)
            .init();
    }

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
        return err.exit_code();
    }

    0
}
