# sv - Simultaneous Versioning

sv is a Rust CLI that makes Git usable for many parallel agents working in the
same repo. It adds workspace-aware coordination (Git worktrees), leases on
paths, protected paths, early conflict signals, and an operation log while
staying Git-native.

**Status**: v0.2 feature-complete. All core coordination features are implemented
and tested.

## Why sv exists

When multiple agents work in one repo, you need fast answers to:

- Who is working on this path right now?
- Is this commit safe to land, or will it collide?
- Which workspace should I rebase onto before merging?
- How do I keep automation safe without freezing the whole repo?

sv provides coordination primitives that sit alongside Git, without requiring a
server or proprietary backend.

## Installation

### Prebuilt binaries (recommended)

GitHub Releases ship binaries for:
- Linux (x86_64)
- macOS (x86_64, arm64)
- Windows (x86_64)

Manual download:
- https://github.com/tOgg1/sv/releases

Linux install script:
```bash
curl -fsSL https://raw.githubusercontent.com/tOgg1/sv/main/install.sh | bash
```
Linux script supports x86_64 only.

macOS Homebrew (tap):
```bash
brew tap trmdy/homebrew-tap
brew install sv
```
Note: Homebrew formula sha256 values are updated automatically on tag releases.

### From source

```bash
git clone https://github.com/tOgg1/sv.git
cd sv
cargo build --release
# Binary at ./target/release/sv
```

### Requirements (source builds)

- Rust 1.70+ (stable)
- Git 2.20+
- libgit2 (bundled via git2 crate)

### System OpenSSL (build requirement)

macOS arm64 (recommended):
```bash
rustup default stable-aarch64-apple-darwin
brew install openssl@3 pkg-config
export OPENSSL_DIR="$(brew --prefix openssl@3)"
export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"
```

macOS x86_64 (only if targeting x86_64):
```bash
# Install x86_64 Homebrew under /usr/local, then:
export OPENSSL_DIR="/usr/local/opt/openssl@3"
export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"
```

Linux:
```bash
# Debian/Ubuntu
sudo apt-get install -y libssl-dev pkg-config
# Fedora/RHEL
sudo dnf install -y openssl-devel pkgconfig
```

## Quickstart (5 minutes)

### 1. Initialize sv in your repo

```bash
cd your-repo
sv init
```

This creates:
- `.sv.toml` - configuration file (tracked)
- `.git/sv/` - shared local state (leases, workspace registry, oplog)
- `.sv/` - per-workspace local state (actor, overrides)

### 2. Set your actor identity

```bash
export SV_ACTOR=alice
# Or persist it:
sv actor set alice
```

### 3. Create a workspace and take a lease

```bash
# Create a new workspace (Git worktree)
sv ws new agent1

# Take a lease on the paths you'll be working on
sv take src/auth/** --strength cooperative --intent feature --note "Auth flow refactor"

# Check your status
sv status
```

### 4. Commit with checks

```bash
# sv commit wraps git commit with coordination checks:
# - Protected path enforcement
# - Lease conflict detection
# - Change-Id trailer injection
sv commit -m "Add auth flow"
```

### 5. Review risk and release

```bash
# See overlap risk across all workspaces
sv risk

# Simulate actual merge conflicts
sv risk --simulate

# Release your leases when done
sv release src/auth/**
```

## Core Concepts

### Workspaces

Workspaces are first-class agent sandboxes built on Git worktrees. Each workspace
has its own directory, branch, and metadata.

```bash
sv ws new agent1                    # Create workspace with branch sv/ws/agent1
sv ws new agent2 --base develop     # Use different base branch
sv ws list                          # List all workspaces
sv ws info agent1                   # Detailed info (branch, ahead/behind, leases)
sv ws switch agent1                 # Print workspace path for quick switching
cd "$(sv ws switch agent1)"         # Switch your shell to that workspace
sv ws switch                        # Select workspace interactively, then print path
sv ws here --name local             # Register current directory as workspace
sv ws rm agent1                     # Remove workspace
sv ws clean --dest main             # Remove merged workspaces
```

### Leases

Leases are graded reservations on paths that signal intent and prevent conflicts.

**Strength levels** (from lowest to highest):
- `observe` - just watching, no conflict with anything
- `cooperative` - working here, overlaps with other cooperative are OK
- `strong` - serious work, overlaps need explicit `--allow-overlap`
- `exclusive` - full ownership, blocks all other leases

**Intent annotations**:
- `bugfix`, `feature`, `docs`, `refactor`, `rename`, `format`, `mechanical`, `investigation`, `other`

```bash
# Take a lease
sv take src/auth/** --strength cooperative --intent bugfix --note "Fix token refresh"
sv take Cargo.lock --strength exclusive --note "Dependency update" --ttl 1h

# View leases
sv lease ls                         # List all active leases
sv lease ls --actor alice           # Filter by actor
sv lease who src/auth/token.rs      # Who has leases on this path?

# Manage leases
sv lease renew <id> --ttl 4h        # Extend TTL
sv lease break <id> --reason "..."  # Emergency override (audited)
sv release src/auth/**              # Release by pathspec
sv release <id>                     # Release by ID
```

### Tasks

Tasks are repo-scoped work items tracked in `.tasks/` and synced across worktrees.
Statuses are configurable in `.sv.toml` (default: `open`, `in_progress`, `closed`).

```bash
# Set a repo prefix for task IDs
sv task prefix acme

# Create + start a task in the current workspace
sv task new "Ship CLI help"
sv task start acme-abc

# Update status + comment
sv task status acme-abc under_review
sv task comment acme-abc "Waiting on QA"
sv task edit acme-abc --title "Ship CLI help v2"

# Parent + relations
sv task parent set acme-abc acme-xyz
sv task epic set acme-def acme-xyz
sv task project set acme-def acme-proj
sv task block acme-xyz acme-def
sv task relate acme-abc acme-ghi --desc "shared refactor"
sv task relations acme-abc

# List tasks (filters)
sv task list --status open
sv task list --epic acme-xyz
sv task list --project acme-proj
sv task list --workspace agent1
sv task list --actor alice --updated-since 2025-01-01T00:00:00Z

# Close + sync history
sv task close acme-abc
sv task delete acme-abc
sv task sync

# Diagnose + repair duplicate create events
sv task doctor
sv task repair --dedupe-creates --dry-run
sv task repair --dedupe-creates
```

### Task log troubleshooting

If `task_created` events were accidentally duplicated for a task ID, task replay
can become inconsistent across worktrees. Use:

- `sv task doctor` to detect duplicates and malformed JSONL lines.
- `sv task repair --dedupe-creates --dry-run` to preview exact removals.
- `sv task repair --dedupe-creates` to remove duplicate create events and rebuild snapshots.

`sv task sync` now warns when duplicate `task_created` events are present.

### Protected Paths

Protected paths are global guardrails that prevent accidental changes to critical files.

**Modes**:
- `guard` (default) - block commits unless `--allow-protected`
- `warn` - emit warning but allow commit
- `readonly` - (future) prevent file modification

```bash
sv protect add .beads/** --mode guard
sv protect add "*.lock" --mode warn
sv protect status                   # Show all rules and staged matches
sv protect off Cargo.lock           # Disable in current workspace only
sv protect rm .beads/**             # Remove from .sv.toml
```

### Risk Assessment

Risk detection finds overlapping work across workspaces before it becomes a merge conflict.

```bash
sv risk                             # Fast overlap detection
sv risk --simulate                  # Virtual merge to find real conflicts
sv risk --json                      # Machine-readable output
```

Output includes:
- Overlapping files with severity (low/medium/high/critical)
- Which workspaces touch each file
- Suggested actions (take lease, rebase onto, pick another task)

### sv onto - Workspace Repositioning

Rebase or merge your workspace onto another workspace's branch.

```bash
sv onto agent5                      # Rebase onto agent5's branch
sv onto agent5 --strategy merge     # Merge instead of rebase
sv onto agent5 --preflight          # Preview conflicts without executing
sv onto agent5 --base develop       # Use custom base ref
```

The `--preflight` flag runs a virtual merge simulation and shows predicted
conflicts before you commit to the operation.

### sv hoist - Bulk Integration

Combine multiple workspace branches into an integration branch.

```bash
# Stack all active workspaces ahead of main
sv hoist -s 'ws(active) & ahead("main")' -d main --strategy stack

# Dry run to see what would happen
sv hoist -s "agent*" -d main --dry-run

# Remove merged workspaces after apply
sv hoist -s 'ws(active)' -d main --rm

# Continue past conflicts, recording them for later
sv hoist -s 'ws(active)' -d main --continue-on-conflict
```

**Strategies**:
- `stack` - cherry-pick commits in order (deduplicates by Change-Id)
- `rebase` - rebase each workspace onto integration branch
- `merge` - merge each workspace branch

**Ordering modes**:
- `workspace` - stable sort by workspace name
- `time` - sort by commit timestamp
- `explicit` - config-defined priority

### Events

sv can emit JSONL events for external integrations (MCP mail, Slack, monitoring).

Integration notes:
- forge loop context hooks: `docs/integrations/forge.md`

```bash
sv take src/auth/** --events                    # Events to stdout
sv take src/auth/** --events /tmp/sv.jsonl      # Events to file
sv release src/auth/** --events -               # Explicit stdout
```

**Event kinds**:
- `lease_created` - emitted by `sv take`
- `lease_released` - emitted by `sv release`
- `workspace_created` - emitted by `sv ws new`
- `workspace_removed` - emitted by `sv ws rm`
- `commit_blocked` - emitted when policy blocks a commit
- `commit_created` - emitted by `sv commit`
- `task_created` - emitted by `sv task new`
- `task_started` - emitted by `sv task start`
- `task_status_changed` - emitted by `sv task status`
- `task_closed` - emitted by `sv task close`
- `task_edited` - emitted by `sv task edit`
- `task_deleted` - emitted by `sv task delete`
- `task_commented` - emitted by `sv task comment`
- `task_epic_set` - emitted by `sv task epic set`
- `task_epic_cleared` - emitted by `sv task epic clear`
- `task_project_set` - emitted by `sv task project set`
- `task_project_cleared` - emitted by `sv task project clear`
- `task_parent_set` - emitted by `sv task parent set`
- `task_parent_cleared` - emitted by `sv task parent clear`
- `task_blocked` - emitted by `sv task block`
- `task_unblocked` - emitted by `sv task unblock`
- `task_related` - emitted by `sv task relate`
- `task_unrelated` - emitted by `sv task unrelate`

Event envelope:
```json
{
  "schema_version": "sv.event.v1",
  "event": "lease_created",
  "timestamp": "2025-01-01T12:00:00Z",
  "actor": "alice",
  "data": { "id": "...", "pathspec": "src/auth/**", ... }
}
```

## Command Reference

### Global Flags

| Flag | Env Var | Description |
|------|---------|-------------|
| `--repo <path>` | `SV_REPO` | Path to repository (defaults to cwd) |
| `--actor <name>` | `SV_ACTOR` | Actor identity for leases and ops |
| `--json` | | Structured JSON output |
| `--events [path]` | | Emit JSONL events (stdout or file) |
| `--quiet` | | Suppress non-essential output |
| `--verbose` | | Extra logging |

Task filters also support `SV_EPIC` and `SV_PROJECT` as defaults for `sv task list`, `sv task ready`, `sv task count`, and `sv task`.

### Commands

| Command | Description |
|---------|-------------|
| `sv init` | Initialize sv in a repository |
| `sv status` | Show current workspace summary |
| `sv actor set\|show` | Manage actor identity |
| `sv ws new\|list\|info\|rm\|clean\|here\|switch` | Workspace management |
| `sv switch` | Resolve workspace path for fast switching |
| `sv take` | Create lease reservations |
| `sv release` | Release leases |
| `sv lease ls\|who\|renew\|break` | Inspect and manage leases |
| `sv protect status\|add\|off\|rm` | Protected path management |
| `sv commit` | Commit with sv checks |
| `sv task new\|list\|ready\|show\|start\|status\|priority\|edit\|close\|delete\|comment\|parent\|epic\|project\|block\|unblock\|relate\|unrelate\|relations\|sync\|compact\|prefix` | Task management |
| `sv risk` | Overlap and conflict analysis |
| `sv onto` | Reposition workspace onto another |
| `sv hoist` | Bulk integration of workspaces |
| `sv op log` | View operation history |
| `sv undo` | Undo recent operation |

Run `sv <command> --help` for detailed usage.

## Output Format

### Human Output

Human output uses a consistent format:
```
sv <command>: <header>

Summary:
  key: value
  ...

Details:
  - item
  ...

Warnings:
  - warning message

Next steps:
  - suggested action
```

### JSON Output

JSON output uses a stable envelope:
```json
{
  "schema_version": "sv.v1",
  "command": "take",
  "status": "success",
  "data": { ... },
  "warnings": ["..."],
  "next_steps": ["..."]
}
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | User error (bad args, missing repo) |
| 3 | Blocked by policy (protected path, lease conflict) |
| 4 | Operation failed (git errors, merge conflicts) |

## Configuration

sv reads `.sv.toml` from the repo root.

```toml
# Base branch for new workspaces
base = "main"

[actor]
# Default actor when SV_ACTOR not set
default = "unknown"

[leases]
# Default lease settings
default_strength = "cooperative"
default_intent = "other"
default_ttl = "2h"
expiration_grace = "0s"
# Require --note for strong/exclusive leases
require_note = true

[leases.compat]
# Allow cooperative leases to overlap
allow_overlap_cooperative = true
# Require --allow-overlap for strong lease overlaps
require_flag_for_strong_overlap = true

[tasks]
id_prefix = "sv"
statuses = ["open", "in_progress", "closed"]
default_status = "open"
in_progress_status = "in_progress"
closed_statuses = ["closed"]

[tasks.compaction]
auto = false
max_log_mb = 200
older_than = "180d"

[protect]
# Default protection mode
mode = "guard"
# Protected path patterns
paths = [".beads/**", "*.lock"]

# Per-path overrides
[[protect.rules]]
path = "Cargo.lock"
mode = "warn"
```

## Storage Layout

```
.sv/                          # Workspace-local state (ignored)
  actor                       # Current actor identity
  workspace.json              # Workspace metadata
  overrides/
    protect.json              # Per-workspace protect overrides

.git/sv/                      # Shared local state (ignored)
  workspaces.json             # Workspace registry
  leases.jsonl                # Lease records
  oplog/                      # Operation log entries
  hoist/                      # Hoist state and conflict records

.tasks/                       # Task log + snapshot (tracked)
  tasks.jsonl
  tasks.snapshot.json

.sv.toml                      # Configuration (tracked)
```

## Selector Language

sv supports a revset-inspired selector language for filtering workspaces:

```
ws(active)                    # All active workspaces
ws(active) & ahead("main")    # Active workspaces with commits ahead of main
name~"agent*"                 # Workspaces matching pattern
touching("src/auth/**")       # Workspaces touching path
blocked                       # Workspaces with lease conflicts

# Operators
a | b                         # Union
a & b                         # Intersection
~a                            # Complement
```

## Development

### Building

```bash
cargo build                   # Debug build
cargo build --release         # Release build
```

### Testing

```bash
cargo test                    # Run all tests
cargo test --test lease       # Run specific test file
cargo test -- --nocapture     # Show println output
```

### Documentation

```bash
cargo doc --open              # Generate and view rustdoc
```

## Architecture

```
src/
  main.rs           # CLI entrypoint
  lib.rs            # Module exports
  cli/              # Clap subcommand handlers
    mod.rs          # CLI structure and routing
    take.rs         # sv take
    release.rs      # sv release
    ws.rs           # sv ws *
    onto.rs         # sv onto
    ...
  actor.rs          # Actor identity management
  config.rs         # .sv.toml parsing
  error.rs          # Error types and exit codes
  events.rs         # JSONL event emission
  git.rs            # git2 wrapper
  hoist.rs          # Hoist logic and Change-Id dedup
  lease.rs          # Lease model and conflict detection
  lock.rs           # File locking primitives
  merge.rs          # Virtual merge simulation
  oplog.rs          # Operation log
  output.rs         # Human + JSON output formatting
  protect.rs        # Protected path logic
  refs.rs           # Branch and ref operations
  risk.rs           # Overlap analysis
  selector.rs       # Selector parsing and evaluation
  storage.rs        # .sv/ and .git/sv/ persistence
  undo.rs           # Undo logic
  workspace.rs      # Workspace management
```

## Related Documentation

- [Product Specification](PRODUCT_SPECIFICATION.md) - Full design document
- [Agent Docs](agent_docs/README.md) - Developer and agent documentation
- [Events Schema](docs/events.md) - Event output format
- [Testing Guide](docs/testing.md) - Test patterns and conventions
- [AGENTS.md](AGENTS.md) - Multi-agent coordination protocol

## License

MIT
