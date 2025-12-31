# sv Architecture

## Overview
sv is a Rust CLI that adds coordination primitives on top of Git for
multi-agent workflows. The high-level goals are:
- workspaces (Git worktrees) as agent sandboxes
- leases to reserve paths with strength/intent/TTL
- protected paths to prevent accidental commits
- risk prediction for overlapping changes
- operation log + undo for safety
- optional event output for integrations

Terminology: this repo uses "workspace" as the primary term; "worktree"
is only used parenthetically to refer to the underlying Git feature.

The product-level design lives in `PRODUCT_SPECIFICATION.md`. This doc
focuses on how that design maps to the codebase.

## Repo layout
- `src/main.rs`: CLI entrypoint, logging setup, and error handling.
- `src/lib.rs`: library root and module exports.
- `src/cli/`: clap-based CLI definitions and command dispatch.
- `src/config.rs`: `.sv.toml` parsing and defaults.
- `src/error.rs`: error types, exit codes, and JSON error wrapper.
- `agent_docs/`: agent runbooks and coordination docs (not user-facing).
- `docs/`: developer documentation (this file).

## Module structure
### `cli`
Defines the CLI shape via clap derive. The `Cli::run()` method is the
central dispatch point for subcommands.

### `config`
Reads `.sv.toml` at the repo root and provides defaults when the file is
missing. The current config schema includes:
- `base`: default base branch (default: `main`)
- `actor.default`: default actor name (default: `unknown`)
- `leases`: default strength/intent/ttl and compat flags
- `protect`: default mode and patterns

### `error`
Defines a crate-level `Error` enum and `Result<T>` alias. Each error
variant maps to a spec-defined exit code:
- 2: user error
- 3: policy block
- 4: operation failed

`JsonError` provides a structured payload for `--json` output.

## Data and storage layout
sv uses three layers of state (per spec):
- Tracked config: `.sv.toml`
- Workspace-local state (ignored): `.sv/`
- Clone-local shared state (ignored): `.git/sv/`

Planned `.git/sv/` contents include:
- `workspaces.json`: workspace registry
- `leases.jsonl` or `leases.sqlite`: active leases + history
- `oplog/`: operation log entries
- `hoist/`: hoist state and conflict records

## Core concepts in code
These concepts are in the spec and are expected to map to modules
as implementation continues:
- **Workspaces (Git worktrees)**: creation/inspection/removal
- **Leases**: graded reservations over pathspecs
- **Protected paths**: commit-time guard/readonly/warn checks
- **Risk prediction**: overlap detection + conflict simulation
- **Change-Id**: commit trailer injection to support hoist
- **Operations + undo**: record ref moves and allow rollback

## Extension points
sv is designed to integrate without a hard dependency on external
coordinators. Expected integration surfaces include:
- Event output stream (`--events`) for lease/workspace/commit events
- Lease storage backend (JSONL vs SQLite) with file-locking
- Risk simulation strategy (fast diff overlap vs merge simulation)

## Testing guide
Test commands are still being defined. Check
`agent_docs/runbooks/test.md` for the authoritative list once populated.
If you add or change tests, update that runbook in the same PR.

## References
- Product spec: `PRODUCT_SPECIFICATION.md`
- CLI scaffolding: `src/cli/mod.rs`
- Config schema: `src/config.rs`
- Error handling: `src/error.rs`
