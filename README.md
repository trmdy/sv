# sv - Simultaneous Versioning

sv is a Rust CLI that makes Git usable for many parallel agents working in the
same repo. It adds workspace-aware coordination (Git worktrees), leases on
paths, protected paths, early conflict signals, and an operation log while
staying Git-native.

Status: early development. The CLI surface is still evolving.

## Why sv exists

When multiple agents work in one repo, you need fast answers to:

- Who is working on this path right now?
- Is this commit safe to land, or will it collide?
- Which workspace should I rebase onto before merging?
- How do I keep automation safe without freezing the whole repo?

sv provides coordination primitives that sit alongside Git, without requiring a
server or proprietary backend.

## Quickstart (5 minutes)

Prereqs: Rust (stable), Git.

```bash
cargo build --release
./target/release/sv init
```

Create a workspace and take a lease:

```bash
export SV_ACTOR=alice
./target/release/sv ws new agent1
./target/release/sv take src/auth/** --strength cooperative --intent feature --ttl 2h
./target/release/sv status
```

Commit with checks, review risk, release leases:

```bash
./target/release/sv commit -m "Add auth flow"
./target/release/sv risk
./target/release/sv release src/auth/**
```

## Core concepts

- Workspaces: first-class workspace sandboxes (Git worktrees) tracked in
  `.git/sv/workspaces.json`. Each workspace has its own branch, base ref, and
  metadata.
- Leases: graded reservations on paths with intent, TTL, and overlap rules.
  Stored in `.git/sv/leases.jsonl` and used for conflict warnings and commit
  blocking.
- Protected paths: global guardrails for critical files (for example `.beads/**`)
  that block or warn on commits.
- Risk prediction: overlap detection across workspaces, with optional merge
  simulation to find real conflicts.
- Change-Id trailers: stable commit identifiers used for hoist/dedup flows.
- Operation log and undo data: append-only records under `.git/sv/oplog/` for
  audit and (future) undo.
- Events: JSONL event model exists; CLI wiring is still in progress.

## CLI at a glance

Global flags:

- `--repo <path>`: path to repo (defaults to cwd)
- `--actor <name>` / `SV_ACTOR`: actor identity for leases and ops
- `--json`: structured JSON output
- `--quiet`: suppress human output
- `--verbose`: extra logging

Implemented commands:

- `sv init`: initialize `.sv/`, `.git/sv/`, `.sv.toml`, and `.gitignore` entries.
- `sv status`: one-pane workspace summary (actor, branch, ahead/behind, leases,
  protected paths).
- `sv ws new|list|info|rm|here`: workspace (worktree) management + registry.
- `sv take`: create leases (recorded in oplog).
- `sv release`: release leases by id or path.
- `sv lease ls|who|renew|break`: inspect, extend, and break leases.
- `sv protect status|add|off|rm`: manage protected paths and per-workspace
  overrides.
- `sv commit`: protected-path and lease checks + Change-Id injection.
- `sv risk [--simulate]`: overlap report and virtual merge simulation.
- `sv op log`: filter and display operation log entries.
- `sv actor set|show`: set or display actor identity.
- `sv hoist`: initialize a hoist integration branch (selection/replay in progress).

Partial or not yet wired:

- `sv undo`: placeholder output only (logic exists but CLI is not wired).
- `sv onto`: not yet wired in the CLI.
- Selector filtering is stubbed in some commands.

Run `sv <cmd> --help` for the latest flags and examples.

## Output contract (human + JSON)

Human output uses a consistent header/summary/detail format. JSON output is a
stable envelope:

```json
{
  "schema_version": "sv.v1",
  "command": "take",
  "status": "success",
  "data": { "...": "..." },
  "warnings": ["..."],
  "next_steps": ["..."]
}
```

Errors use the same envelope with `status: "error"` and include an error code
and optional details. Exit codes are:

- `0`: success
- `2`: user error (bad args, missing repo)
- `3`: blocked by policy (protected path, lease conflict)
- `4`: operation failed (git errors, merge conflicts)

## Configuration (.sv.toml)

sv reads `.sv.toml` from the repo root. Defaults shown:

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

Per-path protection overrides:

```toml
[[protect.paths]]
path = ".beads/**"
mode = "guard"
```

Notes:

- Strong/exclusive leases require a `--note` when `require_note` is true.
- `expiration_grace` keeps expired leases visible for a short window.

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

## Architecture overview

- `src/main.rs`: CLI entrypoint and error handling.
- `src/lib.rs`: module exports.
- `src/cli/`: clap subcommands and handlers.
- `src/storage.rs`: persistence for `.sv/` and `.git/sv/`.
- `src/git.rs`: git2 wrapper helpers.
- `src/lease.rs`: lease model + conflict logic.
- `src/risk.rs`: overlap analysis and merge simulation.
- `src/hoist.rs`: Change-Id dedup and replay helpers.
- `src/output.rs`: human + JSON output envelopes.
- `src/selector.rs`: selector parsing (integration is partial).

## Status and roadmap

Near-term work in this repo includes:

- Event output (`--events`) and finalized JSON schemas.
- Hoist commit selection + replay wiring.
- Selector language integration in list/risk commands.
- Undo command wiring.

## Docs and runbooks

- Product specification: `PRODUCT_SPECIFICATION.md`
- Developer docs index: `agent_docs/README.md`
- Runbooks:
  - `agent_docs/runbooks/dev.md`
  - `agent_docs/runbooks/test.md`
  - `agent_docs/runbooks/release.md`

This repo uses Beads for task tracking and MCP Agent Mail for coordination.
See `AGENTS.md` before starting work.
