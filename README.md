# sv - Simultaneous Versioning

sv is a Rust CLI for coordinating many parallel agents in one Git repo. It
adds workspaces (Git worktrees), leases, protected paths, and early risk
signals while staying Git native.

Status: early development. The CLI surface is evolving. Run `sv --help`
to see what is currently implemented.

## Core concepts

- Workspaces: first-class Git worktrees registered in `.git/sv/workspaces.json`.
- Leases: graded reservations on paths with intent, TTL, and overlap rules.
- Protected paths: guardrails for global files such as `.beads/**`.
- Risk prediction: overlap detection and optional merge simulation.
- Change-Ids: stable commit identifiers used for dedup and hoist workflows.
- Operation log and undo data: append-only records under `.git/sv/oplog/`.
- Events: JSONL event model exists; CLI flag wiring is in progress.

## Architecture overview

- `src/main.rs` wires the CLI and error handling.
- `src/lib.rs` exposes the core modules.
- `src/cli/` holds clap subcommands and their handlers.
- `src/storage.rs` is the persistence layer for `.sv/` and `.git/sv/`.
- `src/git.rs` is a git2 wrapper; `src/merge.rs` handles merge simulation.
- `src/output.rs` centralizes human and JSON output envelopes.
- `src/selector.rs` contains the selector parser and evaluation utilities
  (command integration is still partial).

## Storage layout

```text
.sv/                          # Workspace local (ignored)
  actor                       # Current actor identity
  workspace.json              # Workspace metadata
  overrides/
    protect.json              # Per-workspace protect overrides

.git/sv/                      # Shared local (per clone, ignored)
  workspaces.json             # Workspace registry
  leases.jsonl                # Lease records (JSONL)
  oplog/                      # Operation log entries
  hoist/                      # Hoist state and conflict records
```

## Configuration (.sv.toml)

sv reads `.sv.toml` from the repo root. Current keys (defaults shown):

```toml
base = "main"

[actor]
default = "unknown"

[leases]
default_strength = "cooperative"
default_intent = "other"
default_ttl = "2h"
expiration_grace = "0s"
require_note = true

[leases.compat]
allow_overlap_cooperative = true
require_flag_for_strong_overlap = true

[protect]
mode = "guard"
paths = []
```

Protect paths can use a per-path mode override:

```toml
[[protect.paths]]
path = ".beads/**"
mode = "guard"
```

## Command coverage (current)

Implemented:

- `sv init`: initialize `.sv/`, `.git/sv/`, `.sv.toml`, and `.gitignore`.
- `sv ws new|list|info|rm|here`: worktree management and registry updates.
- `sv take`: create leases (with oplog entries).
- `sv release`: release leases by id or path.
- `sv lease ls|who|break`: inspect and break leases.
- `sv protect status|add|rm`: manage protected paths.
- `sv commit`: protected-path and lease checks + Change-Id injection.
- `sv risk [--simulate]`: overlap report and virtual merge simulation.
- `sv op log`: read and filter the operation log.
- `sv actor set|show`: set or display actor identity.

Partial or stubbed:

- `sv status`: currently a stub.
- `sv lease renew`: stub.
- `sv protect off`: stub (per-workspace disable is not wired).
- `sv undo`: undo logic exists in `src/undo.rs`, CLI not wired.
- `sv hoist`: selection and replay helpers exist, CLI is still wiring.
- Selector filtering flags exist on some commands but are not yet enforced.

## Example workflow

Build and initialize:

```bash
cargo build --release
./target/release/sv init
```

Create a workspace and take a lease:

```bash
export SV_ACTOR=alice
./target/release/sv ws new agent1
./target/release/sv take src/auth/** --strength cooperative --intent feature --ttl 2h
```

Commit with checks and review risk:

```bash
./target/release/sv commit -m "Add auth flow"
./target/release/sv risk
```

Release leases when done:

```bash
./target/release/sv release src/auth/**
```

## Status and roadmap

Near-term work in this repo includes:

- Wire event output (`--events`) and finalize output schemas.
- Implement `sv status` as a single-pane summary.
- Complete `sv hoist` commit selection and replay.
- Add selector evaluation to list and risk commands.

## Docs and runbooks

- Product specification: `PRODUCT_SPECIFICATION.md`
- Developer docs index: `agent_docs/README.md`
- Runbooks:
  - `agent_docs/runbooks/dev.md`
  - `agent_docs/runbooks/test.md`
  - `agent_docs/runbooks/release.md`

This repo uses Beads for task tracking and MCP Agent Mail for coordination.
See `AGENTS.md` before starting work.
