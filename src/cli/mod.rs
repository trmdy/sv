//! Command-line interface for sv
//!
//! This module defines the CLI structure using clap derive macros.
//! Each subcommand is defined in its own submodule.

use clap::{CommandFactory, Parser, Subcommand};

use crate::error::{Error, Result};

mod actor;
mod commit;
mod forge;
mod init;
mod lease;
mod onto;
mod op;
mod project;
mod protect;
mod release;
mod status;
mod switch;
mod take;
mod task;
mod ws;

const ROBOT_HELP: &str = r#"sv --robot-help

Purpose
  sv is a Git-native coordination CLI for multiple agents in one repo. It adds:
  - Workspaces (Git worktrees) as isolated sandboxes
  - Leases on paths to signal intent and reduce collisions
  - Protected paths to guard critical files
  - Risk analysis for overlap/conflict detection
  - Hoist/onto workflows for multi-branch integration

Quickstart (typical agent flow)
  sv init
  sv actor set <name>
  sv ws new <workspace>
  sv take <paths...> --strength cooperative --intent <intent> --note "why"
  sv commit -m "message"
  sv release <paths...>

Environment
  SV_REPO   -> default repo path (otherwise current directory)
  SV_ACTOR  -> default actor name for leases/ops
  SV_ACTOR_FILTER -> default actor filter for task list/ready/count

Storage layout
  .sv.toml           Config (tracked)
  .sv/               Workspace-local state (ignored)
  .sv/worktrees/     Default root for new workspaces (unless --dir is used)
  .git/sv/           Shared local state (leases, registry, oplog, hoist state)
  .tasks/            Task log + snapshot (tracked)

Output contracts
  --json   Machine-readable output with envelope:
           { schema_version, command, status, data, warnings, next_steps }
  --events Emit JSONL events to file or stdout ("-"). Use --events <path> with --json.

Exit codes
  0 success
  2 user error (bad args, missing repo)
  3 blocked by policy (protected paths, lease conflict)
  4 operation failed (git error, merge conflict)

Commands (high level)
  sv init                   Initialize repo state
  sv actor set|show          Configure actor identity
  sv ws new|list|info|rm|clean|here Workspace management
  sv switch                 Resolve workspace path for fast switching
  sv take                   Create leases on paths/globs
  sv release                Release leases
  sv lease ls|who|renew|break|wait Inspect/manage leases
  sv protect status|add|off|rm Protected paths
  sv commit                 Commit with sv checks + Change-Id
  sv task new|list|ready|count|stats|show|start|status|priority|edit|close|delete|comment|parent|epic|project|block|unblock|relate|unrelate|relations|sync|doctor|repair|compact|prefix  Tasks
  sv project new|list|show|edit|archive|unarchive|sync|migrate-legacy  Projects
  sv forge hooks install     Configure Forge task hooks
  sv risk                   Overlap/conflict analysis
  sv onto                   Rebase/merge current workspace onto another
  sv hoist                  Bulk integrate workspaces into an integration branch
  sv op log                 Operation history
  sv undo                   Undo recent ops (limited)

Tasks (notes)
  list/ready sorted: status -> priority -> readiness -> updated_at -> id
  readiness: default_status and not blocked
  TUI list nests children directly under parents

Selectors (for hoist -s)
  ws(active)                Active workspaces
  ahead("main")             Workspaces ahead of main
  name~"agent*"             Name matches pattern
  touching("src/**")        Touching pathspec
  a | b  union, a & b intersection, ~a complement

Leases
  Strength: observe < cooperative < strong < exclusive
  Intent: bugfix, feature, docs, refactor, rename, format, mechanical, investigation, other
  Scope: repo (default), branch:<name>, ws:<workspace>
  TTL: default 2h, configurable in .sv.toml

Protected paths
  Modes: guard (block), warn (allow with warning)
  Per-workspace overrides stored in .sv/overrides/protect.json

Events (JSONL)
  lease_created, lease_released, workspace_created, workspace_removed,
  commit_blocked, commit_created, task_created, task_started,
  task_status_changed, task_priority_changed, task_edited, task_closed, task_deleted,
  task_epic_auto_close_set, task_epic_auto_close_cleared,
  task_commented, task_epic_set, task_epic_cleared, task_project_set, task_project_cleared, task_parent_set, task_parent_cleared, task_blocked,
  task_unblocked, task_related, task_unrelated

Tips for agent automation
  - Use --json for parsing; prefer --events for continuous monitoring.
  - Treat ownerless leases as advisory unless policy says otherwise.
  - Acquire leases before editing paths; release when done.
  - Tasks: if tasks.id_prefix is missing or still "sv", run `sv task prefix <repo>`.
"#;
const WS_ROBOT_HELP: &str = r#"sv ws --robot-help

Purpose
  Workspaces (git worktrees) as sandboxes.

Commands
  sv ws new <name> [--base] [--dir] [--branch] [--sparse ...]
  sv ws here [--name]
  sv ws list [--selector]
  sv ws info <name>
  sv ws rm <name> [--force]
  sv ws clean [--selector] [--dest] [--force] [--dry-run]

Notes
  selector syntax: ws(active), ahead("main"), name~"agent*", touching("src/**")
"#;
const TAKE_ROBOT_HELP: &str = r#"sv take --robot-help

Purpose
  Acquire leases on paths or globs.

Usage
  sv take <paths...> [--strength] [--intent] [--scope] [--ttl] [--note]

Notes
  strength: observe|cooperative|strong|exclusive
  note required for strong/exclusive
"#;
const RELEASE_ROBOT_HELP: &str = r#"sv release --robot-help

Purpose
  Release leases by id or pathspec.

Usage
  sv release <ids|pathspecs...> [--force]
"#;
const LEASE_ROBOT_HELP: &str = r#"sv lease --robot-help

Purpose
  Inspect/manage leases.

Commands
  sv lease ls [--selector] [--actor]
  sv lease who <path>
  sv lease renew <ids...> [--ttl]
  sv lease break <ids...> --reason "<text>"
  sv lease wait <targets...> [--timeout] [--poll]
"#;
const PROTECT_ROBOT_HELP: &str = r#"sv protect --robot-help

Purpose
  Manage protected paths.

Commands
  sv protect status
  sv protect add <patterns...> [--mode guard|readonly|warn]
  sv protect off <patterns...>
  sv protect rm <patterns...> [--force]
"#;
const COMMIT_ROBOT_HELP: &str = r#"sv commit --robot-help

Purpose
  Commit with protected/lease checks + Change-Id injection.

Usage
  sv commit -m "<msg>" [-a] [--amend] [--no-edit] [--allow-protected] [--force-lease]
"#;
const TASK_ROBOT_HELP: &str = r#"sv task --robot-help

Purpose
  Task tracking for this repo. Tasks stored in:
  .tasks/tasks.jsonl (tracked)
  .git/sv/tasks.jsonl (shared across worktrees)

Quickstart
  sv task new "Ship CLI help"
  sv task list --status open
  sv task start 01HZ...
  sv task close 01HZ...

Commands
  sv task [--epic <id>] [--project <id>]  Open task TUI
  sv task new "<title>" [--status] [--priority P0-P4] [--body]
  sv task list [--status] [--priority] [--epic] [--project] [--workspace] [--updated-by] [--updated-since] [--limit]
  sv task ready [--priority] [--epic] [--project] [--workspace] [--updated-by] [--updated-since] [--limit]
  sv task count [--ready] [--status] [--priority] [--epic] [--project] [--workspace] [--updated-by] [--updated-since] [--limit]
  sv task stats
  sv task show <id>
  sv task start <id>
  sv task status <id> <status>
  sv task priority <id> <P0-P4>
  sv task edit <id> [--title] [--body]
  sv task close <id> [--status]
  sv task delete <id>
  sv task comment <id> "<text>"
  sv task parent set <child> <parent>
  sv task parent clear <child>
  sv task epic set <task> <epic>
  sv task epic clear <task>
  sv task epic auto-close <epic> <on|off|inherit>
  sv task project set <task> <project-id>
  sv task project clear <task>
  sv task block <blocker> <blocked>
  sv task unblock <blocker> <blocked>
  sv task relate <left> <right> --desc "<text>"
  sv task unrelate <left> <right>
  sv task relations <id>
  sv task sync
  sv task doctor
  sv task repair --dedupe-creates [--dry-run]
  sv task compact [--older-than] [--max-log-mb] [--dry-run]
  sv task prefix [<prefix>]

Notes
  list/ready sorted: status -> priority -> readiness -> updated_at -> id
  readiness: default_status and not blocked
  epic filter via --epic or SV_EPIC
  project filter via --project or SV_PROJECT
  Use --json for machine output; use --events <path> with --json.
"#;
const PROJECT_ROBOT_HELP: &str = r#"sv project --robot-help

Purpose
  Manage standalone project entities (grouping only; not tasks).

Commands
  sv project new "<name>" [--description]
  sv project list [--all]
  sv project show <id>
  sv project edit <id> [--name] [--description]
  sv project archive <id>
  sv project unarchive <id>
  sv project sync
  sv project migrate-legacy [--dry-run]

Notes
  Legacy task-backed project ids remain readable.
  Use migrate-legacy to create standalone project entities from legacy ids.
"#;
const FORGE_ROBOT_HELP: &str = r#"sv forge --robot-help

Purpose
  Configure sv -> forge hooks that update Forge loop work context on sv task events.

Commands
  sv forge hooks install [--loop <loop-ref>] [--status-map open=in_progress,blocked=blocked,closed=done]

Notes
  Hooks are best-effort: failures never block sv task operations.
"#;
const RISK_ROBOT_HELP: &str = r#"sv risk --robot-help

Purpose
  Overlap/conflict analysis across workspaces.

Usage
  sv risk [--selector] [--base] [--simulate]
"#;
const OP_ROBOT_HELP: &str = r#"sv op --robot-help

Purpose
  Operation history.

Commands
  sv op log [--limit] [--actor] [--operation] [--since] [--until] [--json]
"#;
const UNDO_ROBOT_HELP: &str = r#"sv undo --robot-help

Purpose
  Undo last operation.

Usage
  sv undo [--op <id>]
"#;
const ACTOR_ROBOT_HELP: &str = r#"sv actor --robot-help

Purpose
  Configure actor identity.

Commands
  sv actor set <name>
  sv actor show
"#;
const INIT_ROBOT_HELP: &str = r#"sv init --robot-help

Purpose
  Initialize sv state in repo.

Usage
  sv init
"#;
const STATUS_ROBOT_HELP: &str = r#"sv status --robot-help

Purpose
  Show current workspace status.

Usage
  sv status [--json]
"#;
const SWITCH_ROBOT_HELP: &str = r#"sv switch --robot-help

Purpose
  Resolve workspace path for fast switching.

Usage
  sv switch <name> [--path]
"#;
const ONTO_ROBOT_HELP: &str = r#"sv onto --robot-help

Purpose
  Rebase/merge current workspace onto target workspace.

Usage
  sv onto <target> [--strategy rebase|merge|cherry-pick] [--base] [--preflight]
"#;
const HOIST_ROBOT_HELP: &str = r#"sv hoist --robot-help

Purpose
  Integrate multiple workspaces into an integration branch.

Usage
  sv hoist -s <selector> [--dest] [--strategy] [--order] [--dry-run] [--close-tasks] [--rm] [--rm-force]
"#;

/// sv - Simultaneous Versioning
///
/// A CLI that makes Git practical for many parallel agents by adding
/// workspaces, leases, protected paths, risk prediction, and undo.
#[derive(Parser, Debug)]
#[command(name = "sv")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(subcommand_required = false)]
#[command(after_help = r#"Examples:
  sv init
  sv actor set alice
  sv ws new agent1
  sv take src/auth/** --strength cooperative --intent bugfix --note "Fix refresh edge case"
  sv commit -m "Fix refresh edge case"
  sv risk --json
  sv take src/auth/** --json --events /tmp/sv.events.jsonl

Notes:
  Use --events <path> when combining with --json.
"#)]
pub struct Cli {
    /// Path to the repository (defaults to current directory)
    #[arg(long, global = true, env = "SV_REPO")]
    pub repo: Option<std::path::PathBuf>,

    /// Actor identity for leases and operations
    #[arg(long, global = true, env = "SV_ACTOR")]
    pub actor: Option<String>,

    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,

    /// Emit JSONL events to stdout or a file (use "-" for stdout). Use --events <path> with --json.
    #[arg(long, global = true, value_name = "path", num_args = 0..=1, default_missing_value = "-")]
    pub events: Option<String>,

    /// Print detailed robot-oriented help and exit
    #[arg(long, global = true)]
    pub robot_help: bool,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Workspace management (workspaces)
    #[command(long_about = r#"Manage workspaces used as agent sandboxes.

Examples:
  sv ws new agent1
  sv ws list
  sv ws info agent1
"#)]
    Ws {
        #[command(subcommand)]
        command: Option<WsCommands>,
    },

    /// Take a lease on paths
    #[command(long_about = r#"Create lease reservations on paths or globs.

Examples:
  sv take src/auth/** --strength cooperative --intent bugfix --note "Fix refresh edge case"
  sv take Cargo.lock --strength exclusive --note "Lockfile refresh" --ttl 1h
  sv take src/api/** --scope ws:agent1
  sv take src/auth/** --json --events /tmp/sv.events.jsonl
"#)]
    Take {
        /// Paths to lease (files, directories, or globs)
        #[arg(required = true)]
        paths: Vec<String>,

        /// Lease strength: observe, cooperative, strong, exclusive
        #[arg(long, default_value = "cooperative")]
        strength: String,

        /// Intent: bugfix, feature, docs, refactor, rename, format, mechanical, investigation, other
        #[arg(long, default_value = "other")]
        intent: String,

        /// Scope: repo, branch:<name>, ws:<workspace>
        #[arg(long, default_value = "repo")]
        scope: String,

        /// Time to live (e.g., "2h", "30m")
        #[arg(long, default_value = "2h")]
        ttl: String,

        /// Note explaining the lease (required for strong/exclusive)
        #[arg(long)]
        note: Option<String>,
    },

    /// Release a lease
    #[command(long_about = r#"Release leases by id or pathspec.

Examples:
  sv release 01HZXJ6ZP9QK3A5T
  sv release src/auth/**
  sv release src/auth/** --events /tmp/sv.events.jsonl
"#)]
    Release {
        /// Lease IDs or pathspecs to release
        #[arg(required = true)]
        targets: Vec<String>,

        /// Force release even if owned by another actor
        #[arg(long)]
        force: bool,
    },

    /// Lease management commands
    #[command(long_about = r#"Inspect and manage active leases.

Examples:
  sv lease ls
  sv lease who src/auth/token.rs
  sv lease renew 01HZXJ6ZP9QK3A5T --ttl 4h
  sv lease wait src/auth/** --timeout 10m
"#)]
    Lease {
        #[command(subcommand)]
        command: Option<LeaseCommands>,
    },

    /// Protected path management
    #[command(long_about = r#"Manage protected path rules and overrides.

Examples:
  sv protect status
  sv protect add .beads/** --mode guard
  sv protect off Cargo.lock
"#)]
    Protect {
        #[command(subcommand)]
        command: Option<ProtectCommands>,
    },

    /// Commit with sv checks (protected paths, lease conflicts, Change-Id)
    #[command(
        long_about = r#"Commit with sv checks for protected paths and lease conflicts.

Examples:
  sv commit -m "Fix refresh edge case"
  sv commit --amend --no-edit
  sv commit --allow-protected
  sv commit --force-lease
"#
    )]
    Commit {
        /// Commit message
        #[arg(short, long)]
        message: Option<String>,

        /// Read commit message from file
        #[arg(short = 'F', long)]
        file: Option<std::path::PathBuf>,

        /// Amend the previous commit
        #[arg(long)]
        amend: bool,

        /// Stage all modified files
        #[arg(short, long)]
        all: bool,

        /// Don't open editor (for amend)
        #[arg(long)]
        no_edit: bool,

        /// Allow committing protected paths
        #[arg(long)]
        allow_protected: bool,

        /// Force commit despite lease conflicts
        #[arg(long)]
        force_lease: bool,
    },

    /// Task management
    #[command(long_about = r#"Manage tasks in this repo.

Examples:
  sv task
  sv task new "Ship CLI help"
  sv task list --status open
  sv task start 01HZ...
  sv task close 01HZ...
"#)]
    Task {
        /// Filter tasks by epic ID (or set default via SV_EPIC)
        #[arg(long, env = "SV_EPIC")]
        epic: Option<String>,

        /// Filter tasks by project ID (or set default via SV_PROJECT)
        #[arg(long, env = "SV_PROJECT")]
        project: Option<String>,

        #[command(subcommand)]
        command: Option<TaskCommands>,
    },

    /// Project entity management
    #[command(long_about = r#"Manage standalone project entities.

Examples:
  sv project new "Platform"
  sv project list
  sv project migrate-legacy --dry-run
"#)]
    Project {
        #[command(subcommand)]
        command: Option<ProjectEntityCommands>,
    },

    /// Forge integration helpers
    #[command(long_about = r#"Manage Forge integration helpers.

Examples:
  sv forge hooks install
  sv forge hooks install --loop review-loop
"#)]
    Forge {
        #[command(subcommand)]
        command: Option<ForgeCommands>,
    },

    /// Risk assessment and conflict prediction
    #[command(long_about = r#"Show overlap risk across workspaces.

Examples:
  sv risk
  sv risk --simulate
  sv risk --selector "agent*"
"#)]
    Risk {
        /// Selector for workspaces to analyze
        #[arg(short, long)]
        selector: Option<String>,

        /// Base ref for comparison
        #[arg(long)]
        base: Option<String>,

        /// Simulate actual merge conflicts
        #[arg(long)]
        simulate: bool,
    },

    /// Operation log and undo
    #[command(long_about = r#"Inspect operation history.

Examples:
  sv op log --limit 20
  sv op log --actor alice
"#)]
    Op {
        #[command(subcommand)]
        command: Option<OpCommands>,
    },

    /// Undo the last operation
    #[command(long_about = r#"Undo a recent sv operation.

Examples:
  sv undo
  sv undo --op 01HZXJ6ZP9QK3A5T
"#)]
    Undo {
        /// Specific operation ID to undo
        #[arg(long)]
        op: Option<String>,
    },

    /// Set or show actor identity
    #[command(long_about = r#"Manage the actor identity used for leases and ops.

Examples:
  sv actor set alice
  sv actor show
"#)]
    Actor {
        #[command(subcommand)]
        command: Option<ActorCommands>,
    },

    /// Initialize sv in a repository
    #[command(long_about = r#"Initialize sv state in the repo.

Examples:
  sv init
"#)]
    Init,

    /// Show current workspace status
    #[command(long_about = r#"Show a summary of workspace state.

Examples:
  sv status
  sv status --json
"#)]
    Status,

    /// Resolve workspace path for fast switching
    #[command(long_about = r#"Resolve workspace path for fast switching.

Examples:
  sv switch agent1
  sv switch agent1 --path
"#)]
    Switch {
        /// Workspace name
        name: String,

        /// Print only the workspace path (for `cd $(sv switch <name> --path)`)
        #[arg(long)]
        path: bool,
    },

    /// Reposition current workspace onto another workspace's branch
    #[command(
        long_about = r#"Rebase or merge current workspace onto target workspace.

Examples:
  sv onto agent5
  sv onto agent5 --strategy merge
  sv onto agent5 --base main
  sv onto agent5 --preflight
"#
    )]
    Onto {
        /// Target workspace name to rebase onto
        target: String,

        /// Strategy: rebase (default), merge, or cherry-pick
        #[arg(long, default_value = "rebase")]
        strategy: String,

        /// Base ref for rebase (default: workspace base)
        #[arg(long)]
        base: Option<String>,

        /// Preview conflicts before rebasing (dry run with merge simulation)
        #[arg(long)]
        preflight: bool,
    },

    /// Hoist workspace branches into an integration branch
    #[command(long_about = r#"Initialize a hoist run and integration branch.

Examples:
  sv hoist -s 'ws(active) & ahead("main")' --strategy stack --order workspace
  sv hoist -s "agent*" -d main --dry-run
"#)]
    Hoist {
        /// Selector for workspaces to include (e.g., ws(active) & ahead("main") or legacy actor:agent*)
        #[arg(short, long, required = true)]
        selector: String,

        /// Destination ref to integrate onto (e.g., "main") (default: current branch)
        #[arg(short, long)]
        dest: Option<String>,

        /// Integration strategy: stack, rebase, or merge
        #[arg(long, default_value = "stack")]
        strategy: String,

        /// Ordering mode: workspace, time, or explicit
        #[arg(long, default_value = "workspace")]
        order: String,

        /// Dry run: show what would be done without making changes
        #[arg(long)]
        dry_run: bool,

        /// Continue past conflicts, recording them for later resolution (legacy, use with --no-propagate-conflicts)
        #[arg(long)]
        continue_on_conflict: bool,

        /// Disable jj-style conflict propagation (stop on conflicts instead of committing markers)
        #[arg(long)]
        no_propagate_conflicts: bool,

        /// Skip the final fast-forward merge to dest (only update integration branch)
        #[arg(long)]
        no_apply: bool,

        /// Close active tasks for matching workspaces
        #[arg(long)]
        close_tasks: bool,

        /// Remove matching workspaces after a successful apply
        #[arg(long)]
        rm: bool,

        /// Force workspace removal even with uncommitted changes (implies --rm)
        #[arg(long, requires = "rm")]
        rm_force: bool,
    },
}

/// Workspace subcommands
#[derive(Subcommand, Debug)]
pub enum WsCommands {
    /// Create a new workspace
    #[command(long_about = r#"Create a workspace and register it.

Examples:
  sv ws new agent1
  sv ws new agent1 --base main --dir ../agent1
"#)]
    New {
        /// Workspace name
        name: String,

        /// Base ref to branch from
        #[arg(long)]
        base: Option<String>,

        /// Directory path for the workspace
        #[arg(long)]
        dir: Option<std::path::PathBuf>,

        /// Branch name (default: sv/ws/<name>)
        #[arg(long)]
        branch: Option<String>,

        /// Sparse checkout paths
        #[arg(long)]
        sparse: Vec<String>,
    },

    /// Register current directory as a workspace
    #[command(long_about = r#"Register the current directory as a workspace.

Examples:
  sv ws here --name local
"#)]
    Here {
        /// Workspace name
        #[arg(long)]
        name: Option<String>,
    },

    /// List workspaces
    #[command(
        alias = "ls",
        long_about = r#"List registered workspaces.

Examples:
  sv ws list
  sv ws list -s "agent*"
"#
    )]
    List {
        /// Selector to filter workspaces
        #[arg(short, long)]
        selector: Option<String>,
    },

    /// Show detailed workspace info
    #[command(long_about = r#"Show detailed workspace info.

Examples:
  sv ws info agent1
"#)]
    Info {
        /// Workspace name
        name: String,
    },

    /// Remove a workspace
    #[command(long_about = r#"Remove a workspace and unregister it.

Examples:
  sv ws rm agent1
"#)]
    Rm {
        /// Workspace name
        name: String,

        /// Force removal even with uncommitted changes
        #[arg(long)]
        force: bool,
    },

    /// Remove merged workspaces
    #[command(long_about = r#"Remove merged workspaces in bulk.

Examples:
  sv ws clean --dest main
  sv ws clean -s "ws(active)" --dry-run
"#)]
    Clean {
        /// Selector to filter workspaces (default: ws(active))
        #[arg(short, long)]
        selector: Option<String>,

        /// Destination ref to check merge status against (default: workspace base)
        #[arg(long)]
        dest: Option<String>,

        /// Force removal even with uncommitted changes
        #[arg(long)]
        force: bool,

        /// Dry run: show what would be removed without making changes
        #[arg(long)]
        dry_run: bool,
    },
}

/// Lease subcommands
#[derive(Subcommand, Debug)]
pub enum LeaseCommands {
    /// List active leases
    #[command(long_about = r#"List active leases.

Examples:
  sv lease ls
  sv lease ls --actor alice
"#)]
    Ls {
        /// Selector to filter leases
        #[arg(short, long)]
        selector: Option<String>,

        /// Filter by actor
        #[arg(long)]
        actor: Option<String>,
    },

    /// Show who holds leases on a path
    #[command(long_about = r#"Show leases that overlap a path.

Examples:
  sv lease who src/auth/token.rs
"#)]
    Who {
        /// Path to check
        path: String,
    },

    /// Renew lease TTL
    #[command(long_about = r#"Extend lease expirations.

Examples:
  sv lease renew 01HZXJ6ZP9QK3A5T --ttl 4h
"#)]
    Renew {
        /// Lease IDs to renew
        #[arg(required = true)]
        ids: Vec<String>,

        /// New TTL
        #[arg(long)]
        ttl: Option<String>,
    },

    /// Break a lease (emergency override)
    #[command(long_about = r#"Break a lease and record a reason.

Examples:
  sv lease break 01HZXJ6ZP9QK3A5T --reason "handoff"
"#)]
    Break {
        /// Lease IDs to break
        #[arg(required = true)]
        ids: Vec<String>,

        /// Reason for breaking (required)
        #[arg(long, required = true)]
        reason: String,
    },

    /// Wait for leases to expire
    #[command(long_about = r#"Wait for leases to expire or be released.

Examples:
  sv lease wait 01HZXJ6ZP9QK3A5T
  sv lease wait src/auth/** --timeout 10m
"#)]
    Wait {
        /// Lease IDs or pathspecs to wait on
        #[arg(required = true)]
        targets: Vec<String>,

        /// Max time to wait (e.g., "10m", "30s")
        #[arg(long)]
        timeout: Option<String>,

        /// Poll interval while waiting
        #[arg(long, default_value = "1s")]
        poll: String,
    },
}

/// Protect subcommands
#[derive(Subcommand, Debug)]
pub enum ProtectCommands {
    /// Show protection status
    #[command(long_about = r#"Show protected path rules and overrides.

Examples:
  sv protect status
"#)]
    Status,

    /// Add protected patterns
    #[command(long_about = r#"Add protected patterns to .sv.toml.

Examples:
  sv protect add .beads/** --mode guard
"#)]
    Add {
        /// Patterns to protect
        #[arg(required = true)]
        patterns: Vec<String>,

        /// Protection mode: guard, readonly, warn
        #[arg(long, default_value = "guard")]
        mode: String,
    },

    /// Disable protection for patterns in this workspace
    #[command(long_about = r#"Disable protection for this workspace only.

Examples:
  sv protect off Cargo.lock
"#)]
    Off {
        /// Patterns to disable
        #[arg(required = true)]
        patterns: Vec<String>,
    },

    /// Remove protected patterns from config
    #[command(long_about = r#"Remove protected patterns from .sv.toml.

Examples:
  sv protect rm Cargo.lock
"#)]
    Rm {
        /// Patterns to remove
        #[arg(required = true)]
        patterns: Vec<String>,

        /// Don't error if pattern not found
        #[arg(long)]
        force: bool,
    },
}

/// Task subcommands
#[derive(Subcommand, Debug)]
pub enum TaskCommands {
    /// Create a new task
    #[command(long_about = r#"Create a task.

Examples:
  sv task new "Ship CLI help"
  sv task new "Ship CLI help" --priority P1
"#)]
    New {
        /// Task title
        title: String,

        /// Initial status (defaults to tasks.default_status)
        #[arg(long)]
        status: Option<String>,

        /// Task priority (P0-P4)
        #[arg(long)]
        priority: Option<String>,

        /// Task body/description
        #[arg(long)]
        body: Option<String>,
    },

    /// List tasks
    #[command(long_about = r#"List tasks.

Examples:
  sv task list
  sv task list --status open
  sv task list --priority P2
  sv task list --epic sv-abc
  sv task list --project sv-proj
  sv task list --workspace agent1
  sv task list --updated-by alice --updated-since 2025-01-01T00:00:00Z
  sv task list --limit 20
"#)]
    #[command(visible_alias = "ls")]
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Filter by priority (P0-P4)
        #[arg(long)]
        priority: Option<String>,

        /// Filter by epic task ID
        #[arg(long, env = "SV_EPIC")]
        epic: Option<String>,

        /// Filter by project task ID
        #[arg(long, env = "SV_PROJECT")]
        project: Option<String>,

        /// Filter by workspace (name or id)
        #[arg(long)]
        workspace: Option<String>,

        /// Filter by last updated actor
        #[arg(
            long = "updated-by",
            env = "SV_ACTOR_FILTER",
            id = "task_list_updated_by"
        )]
        actor: Option<String>,

        /// Filter by updated timestamp (RFC3339)
        #[arg(long, value_name = "timestamp")]
        updated_since: Option<String>,

        /// Limit number of tasks returned
        #[arg(long)]
        limit: Option<usize>,
    },

    /// List ready tasks (open and unblocked)
    #[command(long_about = r#"List ready tasks (open and unblocked).

Examples:
  sv task ready
  sv task ready --priority P2
  sv task ready --epic sv-abc
  sv task ready --project sv-proj
  sv task ready --workspace agent1
  sv task ready --updated-by alice --updated-since 2025-01-01T00:00:00Z
  sv task ready --limit 20
"#)]
    Ready {
        /// Filter by priority (P0-P4)
        #[arg(long)]
        priority: Option<String>,

        /// Filter by epic task ID
        #[arg(long, env = "SV_EPIC")]
        epic: Option<String>,

        /// Filter by project task ID
        #[arg(long, env = "SV_PROJECT")]
        project: Option<String>,

        /// Filter by workspace (name or id)
        #[arg(long)]
        workspace: Option<String>,

        /// Filter by last updated actor
        #[arg(
            long = "updated-by",
            env = "SV_ACTOR_FILTER",
            id = "task_ready_updated_by"
        )]
        actor: Option<String>,

        /// Filter by updated timestamp (RFC3339)
        #[arg(long, value_name = "timestamp")]
        updated_since: Option<String>,

        /// Limit number of tasks returned
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Count tasks
    #[command(long_about = r#"Count tasks.

Examples:
  sv task count
  sv task count --status open
  sv task count --priority P2
  sv task count --ready
  sv task count --epic sv-abc
  sv task count --project sv-proj
  sv task count --workspace agent1
  sv task count --updated-by alice --updated-since 2025-01-01T00:00:00Z
"#)]
    Count {
        /// Count ready tasks only (open and unblocked)
        #[arg(long)]
        ready: bool,

        /// Filter by status (not allowed with --ready)
        #[arg(long)]
        status: Option<String>,

        /// Filter by priority (P0-P4)
        #[arg(long)]
        priority: Option<String>,

        /// Filter by epic task ID
        #[arg(long, env = "SV_EPIC")]
        epic: Option<String>,

        /// Filter by project task ID
        #[arg(long, env = "SV_PROJECT")]
        project: Option<String>,

        /// Filter by workspace (name or id)
        #[arg(long)]
        workspace: Option<String>,

        /// Filter by last updated actor
        #[arg(
            long = "updated-by",
            env = "SV_ACTOR_FILTER",
            id = "task_count_updated_by"
        )]
        actor: Option<String>,

        /// Filter by updated timestamp (RFC3339)
        #[arg(long, value_name = "timestamp")]
        updated_since: Option<String>,

        /// Limit number of tasks considered (after filtering)
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Show repo task/project/event stats
    #[command(long_about = r#"Show repository task statistics.

Examples:
  sv task stats
  sv task stats --json
"#)]
    Stats,

    /// Show task details
    #[command(long_about = r#"Show a task by ID.

Examples:
  sv task show 01HZ...
"#)]
    Show {
        /// Task ID
        id: String,
    },

    /// Start a task in the current workspace
    #[command(long_about = r#"Mark a task as in progress.

Examples:
  sv task start 01HZ...
  sv task start 01HZ... --takeover
"#)]
    Start {
        /// Task ID
        id: String,

        /// Take over an in-progress task owned by another actor
        #[arg(long)]
        takeover: bool,
    },

    /// Change task status
    #[command(long_about = r#"Change a task status.

Examples:
  sv task status 01HZ... under_review
"#)]
    Status {
        /// Task ID
        id: String,

        /// New status
        status: String,
    },

    /// Change task priority
    #[command(long_about = r#"Change a task priority.

Examples:
  sv task priority 01HZ... P1
"#)]
    Priority {
        /// Task ID
        id: String,

        /// New priority (P0-P4)
        priority: String,
    },

    /// Edit task fields
    #[command(long_about = r#"Edit task title or body.

Examples:
  sv task edit 01HZ... --title "New title"
  sv task edit 01HZ... --body "New description"
"#)]
    Edit {
        /// Task ID
        id: String,

        /// New title
        #[arg(long)]
        title: Option<String>,

        /// New body/description (use empty string to clear)
        #[arg(long)]
        body: Option<String>,
    },

    /// Close a task
    #[command(long_about = r#"Close a task.

Examples:
  sv task close 01HZ...
"#)]
    Close {
        /// Task ID
        id: String,

        /// Closed status override
        #[arg(long)]
        status: Option<String>,
    },

    /// Delete a task
    #[command(long_about = r#"Delete a task.

Examples:
  sv task delete 01HZ...
"#)]
    Delete {
        /// Task ID
        id: String,
    },

    /// Add a comment
    #[command(long_about = r#"Add a comment to a task.

Examples:
  sv task comment 01HZ... "Follow up with QA"
"#)]
    Comment {
        /// Task ID
        id: String,

        /// Comment text
        text: String,
    },

    /// Manage task parent relationships
    #[command(long_about = r#"Manage task parent relationships.

Examples:
  sv task parent set 01HZ... 01HZ...
  sv task parent clear 01HZ...
"#)]
    Parent {
        #[command(subcommand)]
        command: ParentCommands,
    },

    /// Manage task epic relationships
    #[command(long_about = r#"Manage task epic relationships.

Examples:
  sv task epic set 01HZ... 01HZ...
  sv task epic clear 01HZ...
  sv task epic auto-close 01HZ... on
"#)]
    Epic {
        #[command(subcommand)]
        command: EpicCommands,
    },

    /// Manage task project relationships
    #[command(long_about = r#"Manage task project relationships.

Examples:
  sv task project set 01HZ... 01HZ...
  sv task project clear 01HZ...
"#)]
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Block a task with another task
    #[command(long_about = r#"Record a blocking relationship.

Examples:
  sv task block 01HZ... 01HZ...
"#)]
    Block {
        /// Blocking task ID
        blocker: String,

        /// Blocked task ID
        blocked: String,
    },

    /// Remove a blocking relationship
    #[command(long_about = r#"Remove a blocking relationship.

Examples:
  sv task unblock 01HZ... 01HZ...
"#)]
    Unblock {
        /// Blocking task ID
        blocker: String,

        /// Blocked task ID
        blocked: String,
    },

    /// Relate two tasks with a description
    #[command(long_about = r#"Relate two tasks with a description.

Examples:
  sv task relate 01HZ... 01HZ... --desc "shares context"
"#)]
    Relate {
        /// Left task ID
        left: String,

        /// Right task ID
        right: String,

        /// Relation description
        #[arg(long, required = true)]
        desc: String,
    },

    /// Remove a relation between two tasks
    #[command(long_about = r#"Remove a relation between two tasks.

Examples:
  sv task unrelate 01HZ... 01HZ...
"#)]
    Unrelate {
        /// Left task ID
        left: String,

        /// Right task ID
        right: String,
    },

    /// Show task relationships
    #[command(long_about = r#"Show task relationships.

Examples:
  sv task relations 01HZ...
"#)]
    #[command(visible_alias = "rels")]
    Relations {
        /// Task ID
        id: String,
    },

    /// Sync tracked + shared task logs and snapshots
    #[command(long_about = r#"Merge tracked and shared logs, rebuild snapshot.

Examples:
  sv task sync
"#)]
    Sync,

    /// Validate task logs for replay hazards
    #[command(long_about = r#"Validate task logs for replay hazards.

Examples:
  sv task doctor
  sv task doctor --json
"#)]
    Doctor,

    /// Repair task logs
    #[command(long_about = r#"Repair task logs.

Examples:
  sv task repair --dedupe-creates --dry-run
  sv task repair --dedupe-creates
"#)]
    Repair {
        /// Remove duplicate task_created events (keep earliest by timestamp,event_id)
        #[arg(long)]
        dedupe_creates: bool,

        /// Dry run (no changes)
        #[arg(long)]
        dry_run: bool,
    },

    /// Compact task log
    #[command(long_about = r#"Compact closed task history.

Examples:
  sv task compact --older-than 180d
"#)]
    Compact {
        /// Only compact tasks older than this duration
        #[arg(long)]
        older_than: Option<String>,

        /// Only compact when log exceeds this size (MB)
        #[arg(long)]
        max_log_mb: Option<u64>,

        /// Dry run (no changes)
        #[arg(long)]
        dry_run: bool,
    },

    /// Show or set task ID prefix
    #[command(long_about = r#"Show or set task ID prefix.

Examples:
  sv task prefix
  sv task prefix proj
"#)]
    Prefix {
        /// New prefix (alphanumeric)
        prefix: Option<String>,
    },
}

/// Task parent subcommands
#[derive(Subcommand, Debug)]
pub enum ParentCommands {
    /// Set a parent task
    #[command(long_about = r#"Set a parent task.

Examples:
  sv task parent set 01HZ... 01HZ...
"#)]
    Set {
        /// Child task ID
        child: String,

        /// Parent task ID
        parent: String,
    },

    /// Clear a parent task
    #[command(long_about = r#"Clear a parent task.

Examples:
  sv task parent clear 01HZ...
"#)]
    Clear {
        /// Child task ID
        child: String,
    },
}

/// Task epic subcommands
#[derive(Subcommand, Debug)]
pub enum EpicCommands {
    /// Set task epic
    #[command(long_about = r#"Set task epic.

Examples:
  sv task epic set 01HZ... 01HZ...
"#)]
    Set {
        /// Task ID
        task: String,

        /// Epic task ID
        epic: String,
    },

    /// Clear task epic
    #[command(long_about = r#"Clear task epic.

Examples:
  sv task epic clear 01HZ...
"#)]
    Clear {
        /// Task ID
        task: String,
    },

    /// Configure per-epic auto-close behavior
    #[command(long_about = r#"Set per-epic auto-close behavior.

Examples:
  sv task epic auto-close 01HZ... on
  sv task epic auto-close 01HZ... off
  sv task epic auto-close 01HZ... inherit
"#)]
    AutoClose {
        /// Epic task ID
        epic: String,

        /// Policy mode: on, off, or inherit
        mode: String,
    },
}

/// Task project subcommands
#[derive(Subcommand, Debug)]
pub enum ProjectCommands {
    /// Set task project
    #[command(long_about = r#"Set task project.

Examples:
  sv task project set 01HZ... 01HZ...
"#)]
    Set {
        /// Task ID
        task: String,

        /// Project ID
        project: String,
    },

    /// Clear task project
    #[command(long_about = r#"Clear task project.

Examples:
  sv task project clear 01HZ...
"#)]
    Clear {
        /// Task ID
        task: String,
    },
}

/// Standalone project entity subcommands
#[derive(Subcommand, Debug)]
pub enum ProjectEntityCommands {
    /// Create a project
    New {
        /// Project name
        name: String,

        /// Optional description
        #[arg(long)]
        description: Option<String>,
    },

    /// List projects
    List {
        /// Include archived projects
        #[arg(long)]
        all: bool,
    },

    /// Show a project
    Show {
        /// Project ID (full or prefix)
        id: String,
    },

    /// Edit a project
    Edit {
        /// Project ID
        id: String,

        /// New project name
        #[arg(long)]
        name: Option<String>,

        /// New description (empty string clears)
        #[arg(long)]
        description: Option<String>,
    },

    /// Archive a project
    Archive {
        /// Project ID
        id: String,
    },

    /// Unarchive a project
    Unarchive {
        /// Project ID
        id: String,
    },

    /// Sync project logs and snapshots
    Sync,

    /// Convert legacy task-backed project ids into project entities
    MigrateLegacy {
        /// Show changes without writing
        #[arg(long)]
        dry_run: bool,
    },
}

/// Forge subcommands
#[derive(Subcommand, Debug)]
pub enum ForgeCommands {
    /// Manage .sv.toml hooks
    #[command(long_about = r#"Manage .sv.toml hooks for Forge integration.

Examples:
  sv forge hooks install
"#)]
    Hooks {
        #[command(subcommand)]
        command: Option<ForgeHooksCommands>,
    },
}

/// Forge hook subcommands
#[derive(Subcommand, Debug)]
pub enum ForgeHooksCommands {
    /// Write/update the [integrations.forge] block
    Install {
        /// Forge loop ref (default: "{actor}")
        #[arg(long = "loop")]
        loop_ref: Option<String>,

        /// Map sv categories to forge statuses (default: open=in_progress,blocked=blocked,closed=done)
        #[arg(long = "status-map")]
        status_map: Option<String>,
    },
}

/// Operation log subcommands
#[derive(Subcommand, Debug)]
pub enum OpCommands {
    /// Show operation log
    #[command(long_about = r#"Show recent sv operations.

Examples:
  sv op log --limit 20
  sv op log --actor alice
"#)]
    Log {
        /// Maximum entries to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Filter by actor
        #[arg(long)]
        actor: Option<String>,

        /// Filter by operation type (e.g., "init", "ws", "commit")
        #[arg(long)]
        operation: Option<String>,

        /// Only show entries on/after this RFC3339 timestamp
        #[arg(long)]
        since: Option<String>,

        /// Only show entries on/before this RFC3339 timestamp
        #[arg(long)]
        until: Option<String>,
    },
}

/// Actor subcommands
#[derive(Subcommand, Debug)]
pub enum ActorCommands {
    /// Set actor identity
    #[command(long_about = r#"Persist the actor identity for this workspace.

Examples:
  sv actor set alice
"#)]
    Set {
        /// Actor name
        name: String,
    },

    /// Show current actor
    #[command(long_about = r#"Show the resolved actor identity.

Examples:
  sv actor show
"#)]
    Show,
}

/// Options for risk command
pub struct RiskOptions {
    pub selector: Option<String>,
    pub base: Option<String>,
    pub simulate: bool,
    pub repo: Option<std::path::PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Run risk assessment command
fn run_risk(opts: RiskOptions) -> Result<()> {
    use crate::config::Config;
    use crate::git;
    use crate::risk;

    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let config = Config::load_from_repo(&workdir);

    // Determine base ref
    let base_ref = opts.base.unwrap_or_else(|| config.base.clone());

    if opts.simulate {
        // Run virtual merge simulation
        let report = risk::simulate_conflicts(&repo, &base_ref)?;

        if opts.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else if !opts.quiet {
            print_simulation_report(&report);
        }
    } else {
        // Run basic overlap detection
        let report = risk::compute_risk(&repo, &base_ref)?;

        if opts.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else if !opts.quiet {
            print_risk_report(&report);
        }
    }

    Ok(())
}

fn print_risk_report(report: &crate::risk::RiskReport) {
    println!("Risk Report (base: {})", report.base_ref);
    println!();

    if report.workspaces.is_empty() {
        println!("No workspaces registered.");
        return;
    }

    println!("Workspaces analyzed: {}", report.workspaces.len());
    for ws in &report.workspaces {
        println!(
            "  {} ({}) - {} files touched",
            ws.name,
            ws.branch,
            ws.files.len()
        );
    }
    println!();

    if report.overlaps.is_empty() {
        println!("No overlapping files detected.");
    } else {
        println!("Overlapping files: {}", report.overlaps.len());
        for overlap in &report.overlaps {
            let severity_str = match overlap.severity {
                crate::risk::RiskSeverity::Low => "LOW",
                crate::risk::RiskSeverity::Medium => "MEDIUM",
                crate::risk::RiskSeverity::High => "HIGH",
                crate::risk::RiskSeverity::Critical => "CRITICAL",
            };
            println!(
                "  [{}] {} (workspaces: {})",
                severity_str,
                overlap.path,
                overlap.workspaces.join(", ")
            );
            if !overlap.suggestions.is_empty() {
                for suggestion in &overlap.suggestions {
                    if let Some(command) = &suggestion.command {
                        println!(
                            "    - {}: {} ({})",
                            suggestion.action, suggestion.reason, command
                        );
                    } else {
                        println!("    - {}: {}", suggestion.action, suggestion.reason);
                    }
                }
            }
        }
    }
}

fn print_simulation_report(report: &crate::risk::SimulationReport) {
    println!("Merge Simulation Report (base: {})", report.base_ref);
    println!();

    if report.workspace_pairs.is_empty() {
        println!("No workspace pairs to simulate.");
        return;
    }

    println!("Workspace pairs analyzed: {}", report.workspace_pairs.len());
    println!();

    let mut has_conflicts = false;
    for pair in &report.workspace_pairs {
        if pair.conflicts.is_empty() {
            println!(
                "  {} vs {} - no conflicts",
                pair.workspace_a, pair.workspace_b
            );
        } else {
            has_conflicts = true;
            println!(
                "  {} vs {} - {} conflict(s):",
                pair.workspace_a,
                pair.workspace_b,
                pair.conflicts.len()
            );
            for conflict in &pair.conflicts {
                let kind_str = format!("{:?}", conflict.kind).to_lowercase();
                println!("    [{}] {}", kind_str, conflict.path);
            }
        }
    }

    if !has_conflicts {
        println!();
        println!("All workspace pairs can merge cleanly.");
    }
}

/// Options for hoist command
pub struct HoistOptions {
    pub selector: String,
    pub dest: Option<String>,
    pub strategy: String,
    pub order: String,
    pub dry_run: bool,
    pub continue_on_conflict: bool,
    pub no_propagate_conflicts: bool,
    pub no_apply: bool,
    pub close_tasks: bool,
    pub rm: bool,
    pub rm_force: bool,
    pub actor: Option<String>,
    pub repo: Option<std::path::PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Hoist strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HoistStrategy {
    Stack,
    Rebase,
    Merge,
}

impl std::str::FromStr for HoistStrategy {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stack" => Ok(HoistStrategy::Stack),
            "rebase" => Ok(HoistStrategy::Rebase),
            "merge" => Ok(HoistStrategy::Merge),
            _ => Err(crate::error::Error::InvalidArgument(format!(
                "invalid strategy '{}': must be stack, rebase, or merge",
                s
            ))),
        }
    }
}

/// Ordering mode for hoist
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HoistOrder {
    Workspace,
    Time,
    Explicit,
}

impl std::str::FromStr for HoistOrder {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "workspace" => Ok(HoistOrder::Workspace),
            "time" => Ok(HoistOrder::Time),
            "explicit" => Ok(HoistOrder::Explicit),
            _ => Err(crate::error::Error::InvalidArgument(format!(
                "invalid order '{}': must be workspace, time, or explicit",
                s
            ))),
        }
    }
}

/// Hoist output for JSON
#[derive(Debug, serde::Serialize)]
pub struct HoistOutput {
    pub hoist_id: String,
    pub dest_ref: String,
    pub integration_ref: String,
    pub strategy: HoistStrategy,
    pub order: HoistOrder,
    pub workspaces: Vec<String>,
    pub status: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_on_conflict: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub task_warnings: Vec<HoistTaskWarning>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<HoistConflictSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_cleanup: Option<ws::WorkspaceCleanupReport>,
}

/// Summary of a conflict during hoist
#[derive(Debug, Clone, serde::Serialize)]
pub struct HoistConflictSummary {
    pub commit_id: String,
    pub workspace: String,
    pub files: Vec<String>,
}

/// Summary of active tasks during hoist
#[derive(Debug, Clone, serde::Serialize)]
pub struct HoistTaskWarning {
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

fn resolve_hoist_workspaces(
    repo: &git2::Repository,
    registry: &crate::storage::WorkspacesRegistry,
    selector: &str,
) -> Result<Vec<crate::storage::WorkspaceEntry>> {
    if selector == "all" {
        return Ok(registry.workspaces.clone());
    }

    let parsed = crate::selector::parse_selector(selector);
    if let Ok(expr) = parsed {
        use crate::selector::{EntityKind, Predicate, SelectorContext, SelectorItem};
        use std::collections::HashMap;

        let mut workspace_items = Vec::with_capacity(registry.workspaces.len());
        let mut workspace_lookup = HashMap::new();
        for entry in &registry.workspaces {
            workspace_items.push(SelectorItem::new(entry.name.clone(), entry.name.clone()));
            workspace_lookup.insert(entry.name.clone(), entry);
        }

        let ctx = SelectorContext::new(&workspace_items, &[], &[], |kind, item, predicate| {
            if kind != EntityKind::Workspace {
                return false;
            }
            let entry = match workspace_lookup.get(&item.id) {
                Some(entry) => *entry,
                None => return false,
            };
            match predicate {
                Predicate::Active => entry.path.exists(),
                Predicate::Stale => !entry.path.exists(),
                Predicate::Blocked => false,
                Predicate::Ahead(ref_spec) => workspace_is_ahead(repo, entry, ref_spec),
                Predicate::Touching(pathspec) => workspace_touches(repo, entry, pathspec),
                Predicate::Overlaps(_) => false,
                Predicate::NameMatches(_) => false,
            }
        });

        let matches = crate::selector::evaluate_selector(&expr, &ctx);
        let mut selected = Vec::new();
        for hit in matches {
            if hit.kind != crate::selector::EntityKind::Workspace {
                continue;
            }
            if let Some(entry) = workspace_lookup.get(&hit.item.id) {
                selected.push((*entry).clone());
            }
        }
        return Ok(selected);
    }

    Ok(legacy_match_workspaces(registry, selector))
}

fn legacy_match_workspaces(
    registry: &crate::storage::WorkspacesRegistry,
    selector: &str,
) -> Vec<crate::storage::WorkspaceEntry> {
    registry
        .workspaces
        .iter()
        .filter(|ws| {
            if selector == "all" {
                true
            } else if let Some(prefix) = selector.strip_suffix('*') {
                ws.name.starts_with(prefix)
            } else if let Some(actor_prefix) = selector.strip_prefix("actor:") {
                ws.actor
                    .as_ref()
                    .map(|actor| {
                        if let Some(prefix) = actor_prefix.strip_suffix('*') {
                            actor.starts_with(prefix)
                        } else {
                            actor == actor_prefix
                        }
                    })
                    .unwrap_or(false)
            } else {
                ws.name == selector
            }
        })
        .cloned()
        .collect()
}

fn workspace_is_ahead(
    repo: &git2::Repository,
    entry: &crate::storage::WorkspaceEntry,
    ref_spec: &str,
) -> bool {
    crate::git::commits_ahead(repo, ref_spec, &entry.branch)
        .map(|commits| !commits.is_empty())
        .unwrap_or(false)
}

fn workspace_touches(
    repo: &git2::Repository,
    entry: &crate::storage::WorkspaceEntry,
    pathspec: &str,
) -> bool {
    let changes = match crate::git::diff_files(repo, &entry.base, Some(&entry.branch)) {
        Ok(changes) => changes,
        Err(_) => return false,
    };
    let filtered = crate::git::filter_changes_by_pathspec(changes, &[pathspec.to_string()]);
    !filtered.is_empty()
}

/// Run hoist command
fn run_hoist(opts: HoistOptions) -> Result<()> {
    use crate::actor;
    use crate::config::Config;
    use crate::git;
    use crate::storage::{HoistCommit, HoistState, HoistStatus, Storage};
    use crate::task::{TaskEvent, TaskEventType, TaskStore};
    use chrono::Utc;
    use uuid::Uuid;

    // Parse and validate strategy
    let strategy: HoistStrategy = opts.strategy.parse()?;
    let order: HoistOrder = opts.order.parse()?;

    // Open repository
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = git::common_dir(&repo);
    let storage = Storage::new(workdir.clone(), git_dir, workdir.clone());
    let config = Config::load_from_repo(&workdir);
    let task_store = TaskStore::new(storage.clone(), config.tasks.clone());
    let actor = actor::resolve_actor_optional(Some(&workdir), opts.actor.as_deref())?;

    let dest = match opts.dest {
        Some(dest) => dest,
        None => git::head_info(&repo)
            .ok()
            .and_then(|info| info.shorthand)
            .unwrap_or_else(|| "HEAD".to_string()),
    };

    // Validate dest ref exists
    repo.revparse_single(&dest).map_err(|_| {
        crate::error::Error::InvalidArgument(format!("destination ref '{}' does not exist", dest))
    })?;

    // Get workspaces matching selector
    // For now, we support simple selectors: "all" or prefix matching
    let registry = storage.read_workspaces()?;
    let matching_workspaces = resolve_hoist_workspaces(&repo, &registry, &opts.selector)?;

    if matching_workspaces.is_empty() {
        return Err(crate::error::Error::InvalidArgument(format!(
            "no workspaces match selector '{}'",
            opts.selector
        )));
    }

    let workspace_ids: Vec<String> = matching_workspaces
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    let workspace_names: Vec<String> = matching_workspaces
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    let mut task_warnings: Vec<HoistTaskWarning> = Vec::new();

    let has_task_logs =
        task_store.tracked_log_path().exists() || task_store.shared_log_path().exists();
    if has_task_logs {
        let active_tasks = if opts.dry_run {
            let snapshot = task_store.snapshot_readonly()?;
            let closed: std::collections::HashSet<String> = task_store
                .config()
                .closed_statuses
                .iter()
                .cloned()
                .collect();
            snapshot
                .tasks
                .into_iter()
                .filter(|task| {
                    if closed.contains(&task.status) {
                        return false;
                    }
                    let id_match = task
                        .workspace_id
                        .as_ref()
                        .map(|id| workspace_ids.contains(id))
                        .unwrap_or(false);
                    let name_match = task
                        .workspace
                        .as_ref()
                        .map(|name| workspace_names.contains(name))
                        .unwrap_or(false);
                    id_match || name_match
                })
                .collect::<Vec<_>>()
        } else {
            let policy = task_store.auto_compaction_policy()?;
            let _ = task_store.sync(policy)?;
            task_store.active_tasks_for_workspaces(&workspace_ids, &workspace_names)?
        };

        if !active_tasks.is_empty() {
            task_warnings = active_tasks
                .iter()
                .map(|task| HoistTaskWarning {
                    task_id: task.id.clone(),
                    status: task.status.clone(),
                    workspace: task.workspace.clone(),
                })
                .collect();

            if opts.close_tasks && !opts.dry_run {
                let close_status = task_store
                    .config()
                    .closed_statuses
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "closed".to_string());
                task_store.validate_status(&close_status)?;
                for task in active_tasks {
                    let mut event = TaskEvent::new(TaskEventType::TaskClosed, task.id.clone());
                    event.actor = actor.clone();
                    event.status = Some(close_status.clone());
                    event.workspace_id = task.workspace_id.clone();
                    event.workspace = task.workspace.clone();
                    event.branch = task.branch.clone();
                    task_store.append_event(event)?;
                }
            }
        }
    }

    if !task_warnings.is_empty() && !opts.json && !opts.quiet {
        if opts.close_tasks {
            let verb = if opts.dry_run {
                "Would close"
            } else {
                "Closed"
            };
            println!("{verb} {} task(s):", task_warnings.len());
        } else {
            println!("Active tasks for selected workspaces:");
        }
        for task in &task_warnings {
            let workspace = task.workspace.as_deref().unwrap_or("unknown");
            println!("  - {} ({}, ws: {})", task.task_id, task.status, workspace);
        }
        if !opts.close_tasks {
            println!("  hint: sv task close <id> or sv hoist --close-tasks");
        }
    }

    let explicit_order: Vec<String> = matching_workspaces
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    let order_mode = match order {
        HoistOrder::Workspace => crate::hoist::OrderMode::Workspace,
        HoistOrder::Time => crate::hoist::OrderMode::Time,
        HoistOrder::Explicit => crate::hoist::OrderMode::Explicit(explicit_order),
    };

    let workspace_refs: Vec<crate::hoist::WorkspaceRef> = matching_workspaces
        .iter()
        .map(|entry| crate::hoist::WorkspaceRef {
            name: entry.name.clone(),
            branch: entry.branch.clone(),
        })
        .collect();
    let candidates =
        crate::hoist::select_hoist_commits(&repo, &dest, &workspace_refs, &order_mode)?;
    let hoist_commits = crate::hoist::build_hoist_commits(&repo, &candidates)?;

    // Generate hoist ID and integration branch name
    let hoist_id = Uuid::new_v4().to_string();
    let integration_ref = format!("sv/hoist/{}", dest);

    if opts.dry_run {
        // Dry run output
        let workspace_cleanup = if opts.rm {
            Some(ws::remove_workspaces(
                &workdir,
                &matching_workspaces,
                opts.rm_force,
                true,
                &workdir,
            ))
        } else {
            None
        };
        let output = HoistOutput {
            hoist_id: hoist_id.clone(),
            dest_ref: dest.clone(),
            integration_ref: integration_ref.clone(),
            strategy,
            order,
            workspaces: matching_workspaces.iter().map(|w| w.name.clone()).collect(),
            status: "dry_run".to_string(),
            applied: false,
            continue_on_conflict: if opts.continue_on_conflict {
                Some(true)
            } else {
                None
            },
            task_warnings: task_warnings.clone(),
            conflicts: Vec::new(),
            workspace_cleanup,
        };

        if opts.json {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else if !opts.quiet {
            println!("Hoist (dry run)");
            println!("  ID: {}", hoist_id);
            println!("  Dest: {} -> {}", dest, integration_ref);
            println!("  Strategy: {:?}", strategy);
            println!("  Order: {:?}", order);
            println!("  Workspaces: {}", matching_workspaces.len());
            for ws in &matching_workspaces {
                println!("    - {} ({})", ws.name, ws.branch);
            }
            if let Some(cleanup) = &output.workspace_cleanup {
                println!();
                println!("Workspace cleanup (dry run)");
                if !cleanup.removed.is_empty() {
                    println!("  Would remove: {}", cleanup.removed.join(", "));
                }
                if !cleanup.skipped.is_empty() {
                    println!("  Skipped: {}", cleanup.skipped.len());
                }
                if !cleanup.failed.is_empty() {
                    println!("  Failed: {}", cleanup.failed.len());
                }
            }
        }

        return Ok(());
    }

    // Create or reset integration branch to dest ref
    let dest_commit = repo.revparse_single(&dest)?.peel_to_commit()?;

    // Check if integration branch exists
    let branch_exists = repo
        .find_branch(&integration_ref, git2::BranchType::Local)
        .is_ok();

    if branch_exists {
        // Reset existing branch to dest
        let mut branch = repo.find_branch(&integration_ref, git2::BranchType::Local)?;
        branch
            .get_mut()
            .set_target(dest_commit.id(), &format!("sv hoist: reset to {}", dest))?;
    } else {
        // Create new branch at dest
        repo.branch(&integration_ref, &dest_commit, false)?;
    }

    // Extract commit OIDs for replay
    let commit_oids: Vec<git2::Oid> = candidates.iter().map(|c| c.oid).collect();

    // Replay commits onto the integration branch
    // Default is jj-style propagation (commit conflicts with markers)
    let propagate_conflicts = !opts.no_propagate_conflicts;
    let replay_options = crate::hoist::ReplayOptions {
        continue_on_conflict: opts.continue_on_conflict,
        propagate_conflicts,
    };
    let replay_outcome =
        crate::hoist::replay_commits(&repo, &integration_ref, &commit_oids, &replay_options)?;

    // Build final hoist commits from replay outcome
    let final_commits: Vec<HoistCommit> = replay_outcome
        .entries
        .iter()
        .map(|entry| HoistCommit {
            commit_id: entry.commit_id.to_string(),
            status: entry.status.clone(),
            workspace: hoist_commits
                .iter()
                .find(|c| c.commit_id == entry.commit_id.to_string())
                .and_then(|c| c.workspace.clone()),
            change_id: entry.change_id.clone(),
            summary: entry.summary.clone(),
        })
        .collect();

    // Record conflicts to conflicts.jsonl when propagate_conflicts is enabled
    if propagate_conflicts {
        for entry in &replay_outcome.entries {
            if entry.status == crate::storage::HoistCommitStatus::InConflict {
                if let Some(applied_id) = entry.applied_id {
                    // Find the conflict info for this commit
                    let conflict_files: Vec<String> = replay_outcome
                        .conflicts
                        .iter()
                        .find(|c| c.commit_id == entry.commit_id)
                        .map(|c| c.files.clone())
                        .unwrap_or_default();

                    let record = crate::conflict::ConflictRecord::new(
                        applied_id.to_string(),
                        conflict_files,
                    )
                    .with_hoist_id(&hoist_id)
                    .with_source_commit(entry.commit_id.to_string());

                    storage.append_conflict(&record)?;
                }
            }
        }
    }

    // Determine final status
    let replay_summary = replay_outcome.summary();
    let final_status = if replay_summary.conflicts > 0 {
        // Hard conflicts (not propagated) = failed
        HoistStatus::Failed
    } else if replay_summary.in_conflict > 0 {
        // Propagated conflicts = completed but with conflicts
        HoistStatus::Completed
    } else {
        HoistStatus::Completed
    };

    // Save hoist state
    let now = Utc::now();
    let state = HoistState {
        hoist_id: hoist_id.clone(),
        dest_ref: dest.clone(),
        integration_ref: integration_ref.clone(),
        status: final_status.clone(),
        started_at: now,
        updated_at: now,
        commits: final_commits,
    };
    storage.write_hoist_state(&state)?;

    // Apply: fast-forward dest ref to integration branch
    // Allow apply when:
    // - not --no-apply
    // - no hard conflicts (Conflict status)
    // - something was applied (including in_conflict commits, which were committed)
    let total_applied = replay_summary.applied + replay_summary.in_conflict;
    let refname = if dest.starts_with("refs/") {
        dest.clone()
    } else {
        format!("refs/heads/{}", dest)
    };
    let mut dest_is_head = false;
    let mut worktree_updated = false;
    let mut worktree_update_warning: Option<String> = None;
    let applied = if !opts.no_apply && replay_summary.conflicts == 0 && total_applied > 0 {
        // Get the current tip of the integration branch
        let integration_commit = repo.revparse_single(&integration_ref)?.peel_to_commit()?;

        // Update the dest ref to point to the integration branch tip
        repo.reference(
            &refname,
            integration_commit.id(),
            true,
            &format!("sv hoist: fast-forward {} to {}", dest, integration_ref),
        )?;

        true
    } else {
        false
    };
    if applied {
        if let Ok(head_info) = git::head_info(&repo) {
            dest_is_head = head_info.name.as_deref() == Some(refname.as_str());
            if dest_is_head {
                if git::has_uncommitted_changes(&repo)? {
                    worktree_update_warning =
                        Some("worktree has local changes; not updated".to_string());
                } else {
                    let mut checkout = git2::build::CheckoutBuilder::new();
                    checkout.safe();
                    if let Err(err) = repo.checkout_head(Some(&mut checkout)) {
                        worktree_update_warning = Some(format!("worktree update failed: {}", err));
                    } else {
                        worktree_updated = true;
                    }
                }
            }
        }
    }

    // Build conflict output
    let conflict_output: Vec<HoistConflictSummary> = replay_outcome
        .conflicts
        .iter()
        .map(|c| {
            // Find workspace for this commit
            let workspace = hoist_commits
                .iter()
                .find(|hc| hc.commit_id == c.commit_id.to_string())
                .and_then(|hc| hc.workspace.clone())
                .unwrap_or_else(|| "unknown".to_string());
            HoistConflictSummary {
                commit_id: c.commit_id.to_string(),
                workspace,
                files: c.files.clone(),
            }
        })
        .collect();

    let workspace_cleanup = if opts.rm {
        if replay_summary.conflicts > 0 || replay_summary.in_conflict > 0 {
            let mut report = ws::WorkspaceCleanupReport::new(false);
            report.skipped = matching_workspaces
                .iter()
                .map(|ws| ws::WorkspaceCleanupSkip {
                    name: ws.name.clone(),
                    reason: "hoist has conflicts".to_string(),
                })
                .collect();
            Some(report)
        } else if !applied {
            let reason = if opts.no_apply {
                "hoist not applied (--no-apply)"
            } else if total_applied == 0 {
                "nothing applied"
            } else {
                "hoist not applied"
            };
            let mut report = ws::WorkspaceCleanupReport::new(false);
            report.skipped = matching_workspaces
                .iter()
                .map(|ws| ws::WorkspaceCleanupSkip {
                    name: ws.name.clone(),
                    reason: reason.to_string(),
                })
                .collect();
            Some(report)
        } else {
            Some(ws::remove_workspaces(
                &workdir,
                &matching_workspaces,
                opts.rm_force,
                false,
                &workdir,
            ))
        }
    } else {
        None
    };

    // Output result
    let status_str = match final_status {
        HoistStatus::Completed => "complete",
        HoistStatus::Failed => "failed",
        HoistStatus::InProgress => "in_progress",
    };

    let output = HoistOutput {
        hoist_id: hoist_id.clone(),
        dest_ref: dest.clone(),
        integration_ref: integration_ref.clone(),
        strategy,
        order,
        workspaces: matching_workspaces.iter().map(|w| w.name.clone()).collect(),
        status: status_str.to_string(),
        applied,
        continue_on_conflict: if opts.continue_on_conflict {
            Some(true)
        } else {
            None
        },
        task_warnings: task_warnings.clone(),
        conflicts: conflict_output.clone(),
        workspace_cleanup: workspace_cleanup.clone(),
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        println!("Hoist complete");
        println!("  ID: {}", hoist_id);
        println!("  Integration branch: {}", integration_ref);
        println!("  Base: {} ({})", dest, &dest_commit.id().to_string()[..8]);
        println!("  Strategy: {:?}", strategy);
        println!("  Order: {:?}", order);
        if opts.continue_on_conflict {
            println!("  Continue on conflict: yes");
        }
        println!("  Workspaces: {}", matching_workspaces.len());
        for ws in &matching_workspaces {
            println!("    - {} ({})", ws.name, ws.branch);
        }
        println!();
        println!("Replay summary:");
        println!("  Applied: {}", replay_summary.applied);
        if replay_summary.in_conflict > 0 {
            println!(
                "  In-conflict: {} (committed with markers)",
                replay_summary.in_conflict
            );
        }
        if replay_summary.conflicts > 0 {
            println!("  Conflicts: {} (stopped)", replay_summary.conflicts);
        }
        if replay_summary.skipped > 0 {
            println!("  Skipped: {}", replay_summary.skipped);
        }
        if !conflict_output.is_empty() {
            println!();
            if replay_summary.in_conflict > 0 {
                println!("In-conflict commits (resolve with 'sv resolve'):");
            } else {
                println!("Conflicts:");
            }
            for conflict in &conflict_output {
                println!(
                    "  {} - files: {}",
                    &conflict.commit_id[..8],
                    conflict.files.join(", ")
                );
            }
        }
        println!();
        if applied {
            let commit_count = replay_summary.applied + replay_summary.in_conflict;
            if replay_summary.in_conflict > 0 {
                println!(
                    "{} updated to include {} commit(s) ({} with conflicts)",
                    dest, commit_count, replay_summary.in_conflict
                );
            } else {
                println!("{} updated to include {} commit(s)", dest, commit_count);
            }
            if dest_is_head {
                if worktree_updated {
                    println!("  Worktree: updated");
                } else if let Some(warning) = &worktree_update_warning {
                    println!("  Worktree: not updated ({warning})");
                }
            }
        } else if opts.no_apply {
            println!(
                "Skipped apply (--no-apply). To apply: git checkout {} && git merge --ff-only {}",
                dest, integration_ref
            );
        } else if replay_summary.conflicts > 0 {
            println!("Apply skipped due to conflicts. Resolve conflicts and retry.");
        } else if total_applied == 0 {
            println!("Nothing to apply (no commits replayed).");
        }
        if let Some(cleanup) = &workspace_cleanup {
            println!();
            let header = if cleanup.dry_run {
                "Workspace cleanup (dry run)"
            } else {
                "Workspace cleanup"
            };
            println!("{header}");
            println!("  Removed: {}", cleanup.removed.len());
            println!("  Skipped: {}", cleanup.skipped.len());
            println!("  Failed: {}", cleanup.failed.len());
            if !cleanup.removed.is_empty() {
                let label = if cleanup.dry_run {
                    "Would remove"
                } else {
                    "Removed"
                };
                println!("{label}: {}", cleanup.removed.join(", "));
            }
            if !cleanup.skipped.is_empty() {
                println!("Skipped:");
                for skip in &cleanup.skipped {
                    println!("  - {} ({})", skip.name, skip.reason);
                }
            }
            if !cleanup.failed.is_empty() {
                println!("Failed:");
                for failure in &cleanup.failed {
                    println!("  - {} ({})", failure.name, failure.error);
                }
            }
        }
    }

    Ok(())
}

impl Cli {
    /// Execute the CLI command
    pub fn run(self) -> Result<()> {
        let Cli {
            repo,
            actor,
            json,
            events,
            robot_help,
            quiet,
            verbose: _,
            command,
        } = self;

        if robot_help {
            let help = match &command {
                Some(Commands::Ws { .. }) => WS_ROBOT_HELP,
                Some(Commands::Take { .. }) => TAKE_ROBOT_HELP,
                Some(Commands::Release { .. }) => RELEASE_ROBOT_HELP,
                Some(Commands::Lease { .. }) => LEASE_ROBOT_HELP,
                Some(Commands::Protect { .. }) => PROTECT_ROBOT_HELP,
                Some(Commands::Commit { .. }) => COMMIT_ROBOT_HELP,
                Some(Commands::Task { .. }) => TASK_ROBOT_HELP,
                Some(Commands::Project { .. }) => PROJECT_ROBOT_HELP,
                Some(Commands::Forge { .. }) => FORGE_ROBOT_HELP,
                Some(Commands::Risk { .. }) => RISK_ROBOT_HELP,
                Some(Commands::Op { .. }) => OP_ROBOT_HELP,
                Some(Commands::Undo { .. }) => UNDO_ROBOT_HELP,
                Some(Commands::Actor { .. }) => ACTOR_ROBOT_HELP,
                Some(Commands::Init) => INIT_ROBOT_HELP,
                Some(Commands::Status) => STATUS_ROBOT_HELP,
                Some(Commands::Switch { .. }) => SWITCH_ROBOT_HELP,
                Some(Commands::Onto { .. }) => ONTO_ROBOT_HELP,
                Some(Commands::Hoist { .. }) => HOIST_ROBOT_HELP,
                None => ROBOT_HELP,
            };
            println!("{help}");
            return Ok(());
        }

        let events_to_stdout = matches!(events.as_deref(), Some("-"));
        if events_to_stdout && json {
            return Err(Error::InvalidArgument(
                "--json requires --events <path> to avoid mixing JSON output with JSONL events"
                    .to_string(),
            ));
        }
        let command = match command {
            Some(command) => command,
            None => {
                let mut cli = Cli::command();
                cli.print_help()?;
                println!();
                return Err(Error::InvalidArgument("missing command".to_string()));
            }
        };

        match command {
            Commands::Init => init::run(repo, json, quiet),
            Commands::Status => status::run(status::StatusOptions {
                repo,
                actor,
                json,
                quiet,
            }),
            Commands::Switch { name, path } => switch::run(switch::SwitchOptions {
                name,
                path_only: path,
                repo,
                json,
                quiet,
            }),
            Commands::Onto {
                target,
                strategy,
                base,
                preflight,
            } => onto::run(onto::OntoOptions {
                target_workspace: target,
                strategy,
                base,
                preflight,
                actor,
                repo,
                json,
                quiet,
            }),
            Commands::Ws { command } => match command {
                Some(cmd) => match cmd {
                    WsCommands::New {
                        name,
                        base,
                        dir,
                        branch,
                        sparse,
                    } => ws::run_new(ws::NewOptions {
                        name,
                        base,
                        dir,
                        branch,
                        sparse,
                        actor,
                        repo,
                        json,
                        quiet,
                    }),
                    WsCommands::Here { name } => ws::run_here(ws::HereOptions {
                        name,
                        actor,
                        repo,
                        json,
                        quiet,
                    }),
                    WsCommands::List { selector } => ws::run_list(ws::ListOptions {
                        selector,
                        repo,
                        json,
                        quiet,
                    }),
                    WsCommands::Info { name } => ws::run_info(ws::InfoOptions {
                        name,
                        repo,
                        json,
                        quiet,
                    }),
                    WsCommands::Rm { name, force } => ws::run_rm(ws::RmOptions {
                        name,
                        force,
                        repo,
                        json,
                        quiet,
                    }),
                    WsCommands::Clean {
                        selector,
                        dest,
                        force,
                        dry_run,
                    } => ws::run_clean(ws::CleanOptions {
                        selector,
                        dest,
                        force,
                        dry_run,
                        repo,
                        json,
                        quiet,
                    }),
                },
                None => {
                    print_subcommand_help("ws")?;
                    Err(Error::InvalidArgument("missing ws command".to_string()))
                }
            },
            Commands::Take {
                paths,
                strength,
                intent,
                scope,
                ttl,
                note,
            } => take::run(take::TakeOptions {
                paths,
                strength,
                intent,
                scope,
                ttl,
                note,
                actor,
                events: events.clone(),
                repo,
                json,
                quiet,
            }),
            Commands::Release { targets, force } => release::run(release::ReleaseOptions {
                targets,
                actor,
                events: events.clone(),
                repo,
                force,
                json,
                quiet,
            }),
            Commands::Lease { command } => match command {
                Some(cmd) => match cmd {
                    LeaseCommands::Ls { selector, actor } => lease::run_ls(lease::LsOptions {
                        selector,
                        actor,
                        repo,
                        json,
                        quiet,
                    }),
                    LeaseCommands::Who { path } => lease::run_who(lease::WhoOptions {
                        path,
                        repo,
                        json,
                        quiet,
                    }),
                    LeaseCommands::Renew { ids, ttl } => lease::run_renew(lease::RenewOptions {
                        ids,
                        ttl,
                        actor,
                        repo,
                        json,
                        quiet,
                    }),
                    LeaseCommands::Break { ids, reason } => lease::run_break(lease::BreakOptions {
                        ids,
                        reason,
                        actor,
                        repo,
                        json,
                        quiet,
                    }),
                    LeaseCommands::Wait {
                        targets,
                        timeout,
                        poll,
                    } => lease::run_wait(lease::WaitOptions {
                        targets,
                        timeout,
                        poll,
                        repo,
                        json,
                        quiet,
                    }),
                },
                None => {
                    print_subcommand_help("lease")?;
                    Err(Error::InvalidArgument("missing lease command".to_string()))
                }
            },
            Commands::Protect { command } => match command {
                Some(cmd) => match cmd {
                    ProtectCommands::Status => {
                        protect::run_status(protect::StatusOptions { repo, json, quiet })
                    }
                    ProtectCommands::Add { patterns, mode } => {
                        protect::run_add(protect::AddOptions {
                            patterns,
                            mode,
                            repo,
                            json,
                            quiet,
                        })
                    }
                    ProtectCommands::Off { patterns } => protect::run_off(protect::OffOptions {
                        patterns,
                        repo,
                        json,
                        quiet,
                    }),
                    ProtectCommands::Rm { patterns, force } => {
                        protect::run_rm(protect::RmOptions {
                            patterns,
                            force,
                            repo,
                            json,
                            quiet,
                        })
                    }
                },
                None => {
                    print_subcommand_help("protect")?;
                    Err(Error::InvalidArgument(
                        "missing protect command".to_string(),
                    ))
                }
            },
            Commands::Commit {
                message,
                file,
                amend,
                all,
                no_edit,
                allow_protected,
                force_lease,
            } => commit::run(commit::CommitOptions {
                message,
                file,
                amend,
                all,
                no_edit,
                allow_protected,
                force_lease,
                actor,
                repo,
                json,
                quiet,
            }),
            Commands::Task {
                epic,
                project,
                command,
            } => match command {
                Some(cmd) => match cmd {
                    TaskCommands::New {
                        title,
                        status,
                        priority,
                        body,
                    } => task::run_new(task::NewOptions {
                        title,
                        status,
                        priority,
                        body,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::List {
                        status,
                        priority,
                        epic: list_epic,
                        project: list_project,
                        workspace,
                        actor: list_actor,
                        updated_since,
                        limit,
                    } => task::run_list(task::ListOptions {
                        status,
                        priority,
                        epic: list_epic.or_else(|| epic.clone()),
                        project: list_project.or_else(|| project.clone()),
                        workspace,
                        actor: list_actor,
                        updated_since,
                        limit,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Ready {
                        priority,
                        epic: list_epic,
                        project: list_project,
                        workspace,
                        actor: list_actor,
                        updated_since,
                        limit,
                    } => task::run_ready(task::ReadyOptions {
                        priority,
                        epic: list_epic.or_else(|| epic.clone()),
                        project: list_project.or_else(|| project.clone()),
                        workspace,
                        actor: list_actor,
                        updated_since,
                        limit,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Count {
                        ready,
                        status,
                        priority,
                        epic: list_epic,
                        project: list_project,
                        workspace,
                        actor: list_actor,
                        updated_since,
                        limit,
                    } => task::run_count(task::CountOptions {
                        ready,
                        status,
                        priority,
                        epic: list_epic.or_else(|| epic.clone()),
                        project: list_project.or_else(|| project.clone()),
                        workspace,
                        actor: list_actor,
                        updated_since,
                        limit,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Stats => {
                        task::run_stats(task::StatsOptions { repo, json, quiet })
                    }
                    TaskCommands::Show { id } => task::run_show(task::ShowOptions {
                        id,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Start { id, takeover } => task::run_start(task::StartOptions {
                        id,
                        takeover,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Status { id, status } => task::run_status(task::StatusOptions {
                        id,
                        status,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Priority { id, priority } => {
                        task::run_priority(task::PriorityOptions {
                            id,
                            priority,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        })
                    }
                    TaskCommands::Edit { id, title, body } => task::run_edit(task::EditOptions {
                        id,
                        title,
                        body,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Close { id, status } => task::run_close(task::CloseOptions {
                        id,
                        status,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Delete { id } => task::run_delete(task::DeleteOptions {
                        id,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Comment { id, text } => task::run_comment(task::CommentOptions {
                        id,
                        text,
                        actor,
                        events: events.clone(),
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Parent { command } => match command {
                        ParentCommands::Set { child, parent } => {
                            task::run_parent_set(task::ParentSetOptions {
                                child,
                                parent,
                                actor,
                                events: events.clone(),
                                repo,
                                json,
                                quiet,
                            })
                        }
                        ParentCommands::Clear { child } => {
                            task::run_parent_clear(task::ParentClearOptions {
                                child,
                                actor,
                                events: events.clone(),
                                repo,
                                json,
                                quiet,
                            })
                        }
                    },
                    TaskCommands::Epic { command } => match command {
                        EpicCommands::Set {
                            task: task_id,
                            epic,
                        } => task::run_epic_set(task::EpicSetOptions {
                            task: task_id,
                            epic,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        }),
                        EpicCommands::Clear { task: task_id } => {
                            task::run_epic_clear(task::EpicClearOptions {
                                task: task_id,
                                actor,
                                events: events.clone(),
                                repo,
                                json,
                                quiet,
                            })
                        }
                        EpicCommands::AutoClose { epic, mode } => {
                            task::run_epic_auto_close(task::EpicAutoCloseOptions {
                                epic,
                                mode,
                                actor,
                                events: events.clone(),
                                repo,
                                json,
                                quiet,
                            })
                        }
                    },
                    TaskCommands::Project { command } => match command {
                        ProjectCommands::Set {
                            task: task_id,
                            project,
                        } => task::run_project_set(task::ProjectSetOptions {
                            task: task_id,
                            project,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        }),
                        ProjectCommands::Clear { task: task_id } => {
                            task::run_project_clear(task::ProjectClearOptions {
                                task: task_id,
                                actor,
                                events: events.clone(),
                                repo,
                                json,
                                quiet,
                            })
                        }
                    },
                    TaskCommands::Block { blocker, blocked } => {
                        task::run_block(task::BlockOptions {
                            blocker,
                            blocked,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        })
                    }
                    TaskCommands::Unblock { blocker, blocked } => {
                        task::run_unblock(task::UnblockOptions {
                            blocker,
                            blocked,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        })
                    }
                    TaskCommands::Relate { left, right, desc } => {
                        task::run_relate(task::RelateOptions {
                            left,
                            right,
                            description: desc,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        })
                    }
                    TaskCommands::Unrelate { left, right } => {
                        task::run_unrelate(task::UnrelateOptions {
                            left,
                            right,
                            actor,
                            events: events.clone(),
                            repo,
                            json,
                            quiet,
                        })
                    }
                    TaskCommands::Relations { id } => task::run_relations(task::RelationsOptions {
                        id,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Sync => task::run_sync(task::SyncOptions { repo, json, quiet }),
                    TaskCommands::Doctor => {
                        task::run_doctor(task::DoctorOptions { repo, json, quiet })
                    }
                    TaskCommands::Repair {
                        dedupe_creates,
                        dry_run,
                    } => task::run_repair(task::RepairOptions {
                        dedupe_creates,
                        dry_run,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Compact {
                        older_than,
                        max_log_mb,
                        dry_run,
                    } => task::run_compact(task::CompactOptions {
                        older_than,
                        max_log_mb,
                        dry_run,
                        repo,
                        json,
                        quiet,
                    }),
                    TaskCommands::Prefix { prefix } => task::run_prefix(task::PrefixOptions {
                        prefix,
                        repo,
                        json,
                        quiet,
                    }),
                },
                None => task::run_tui(task::TuiOptions {
                    epic,
                    project,
                    repo,
                    json,
                    quiet,
                }),
            },
            Commands::Project { command } => match command {
                Some(cmd) => match cmd {
                    ProjectEntityCommands::New { name, description } => {
                        project::run_new(project::NewOptions {
                            name,
                            description,
                            actor,
                            repo,
                            json,
                            quiet,
                        })
                    }
                    ProjectEntityCommands::List { all } => {
                        project::run_list(project::ListOptions {
                            all,
                            repo,
                            json,
                            quiet,
                        })
                    }
                    ProjectEntityCommands::Show { id } => project::run_show(project::ShowOptions {
                        id,
                        repo,
                        json,
                        quiet,
                    }),
                    ProjectEntityCommands::Edit {
                        id,
                        name,
                        description,
                    } => project::run_edit(project::EditOptions {
                        id,
                        name,
                        description,
                        actor,
                        repo,
                        json,
                        quiet,
                    }),
                    ProjectEntityCommands::Archive { id } => {
                        project::run_archive(project::ArchiveOptions {
                            id,
                            actor,
                            repo,
                            json,
                            quiet,
                        })
                    }
                    ProjectEntityCommands::Unarchive { id } => {
                        project::run_unarchive(project::UnarchiveOptions {
                            id,
                            actor,
                            repo,
                            json,
                            quiet,
                        })
                    }
                    ProjectEntityCommands::Sync => {
                        project::run_sync(project::SyncOptions { repo, json, quiet })
                    }
                    ProjectEntityCommands::MigrateLegacy { dry_run } => {
                        project::run_migrate_legacy(project::MigrateLegacyOptions {
                            dry_run,
                            actor,
                            repo,
                            json,
                            quiet,
                        })
                    }
                },
                None => {
                    print_subcommand_help("project")?;
                    Err(Error::InvalidArgument(
                        "missing project command".to_string(),
                    ))
                }
            },
            Commands::Forge { command } => match command {
                Some(cmd) => match cmd {
                    ForgeCommands::Hooks { command } => match command {
                        Some(ForgeHooksCommands::Install {
                            loop_ref,
                            status_map,
                        }) => forge::run_hooks_install(forge::HooksInstallOptions {
                            loop_ref,
                            status_map,
                            repo,
                            json,
                            quiet,
                        }),
                        None => {
                            print_subcommand_help("forge")?;
                            Err(Error::InvalidArgument(
                                "missing forge hooks command".to_string(),
                            ))
                        }
                    },
                },
                None => {
                    print_subcommand_help("forge")?;
                    Err(Error::InvalidArgument("missing forge command".to_string()))
                }
            },
            Commands::Risk {
                selector,
                base,
                simulate,
            } => run_risk(RiskOptions {
                selector,
                base,
                simulate,
                repo,
                json,
                quiet,
            }),
            Commands::Op { command } => match command {
                Some(cmd) => match cmd {
                    OpCommands::Log {
                        limit,
                        actor,
                        operation,
                        since,
                        until,
                    } => op::run_log(op::LogOptions {
                        limit,
                        actor,
                        operation,
                        since,
                        until,
                        repo,
                        json,
                        quiet,
                    }),
                },
                None => {
                    print_subcommand_help("op")?;
                    Err(Error::InvalidArgument("missing op command".to_string()))
                }
            },
            Commands::Undo { op } => {
                if !quiet {
                    println!("sv undo {:?} - not yet implemented", op);
                }
                Ok(())
            }
            Commands::Actor { command } => match command {
                Some(cmd) => match cmd {
                    ActorCommands::Set { name } => actor::run_set(actor::SetOptions {
                        name,
                        repo,
                        json,
                        quiet,
                    }),
                    ActorCommands::Show => actor::run_show(actor::ShowOptions {
                        repo,
                        actor,
                        json,
                        quiet,
                    }),
                },
                None => {
                    print_subcommand_help("actor")?;
                    Err(Error::InvalidArgument("missing actor command".to_string()))
                }
            },
            Commands::Hoist {
                selector,
                dest,
                strategy,
                order,
                dry_run,
                continue_on_conflict,
                no_propagate_conflicts,
                no_apply,
                close_tasks,
                rm,
                rm_force,
            } => run_hoist(HoistOptions {
                selector,
                dest,
                strategy,
                order,
                dry_run,
                continue_on_conflict,
                no_propagate_conflicts,
                no_apply,
                close_tasks,
                rm,
                rm_force,
                actor,
                repo,
                json,
                quiet,
            }),
        }
    }
}

fn print_subcommand_help(name: &str) -> Result<()> {
    let mut cli = Cli::command();
    if let Some(subcommand) = cli.find_subcommand_mut(name) {
        subcommand.print_help()?;
        println!();
    }
    Ok(())
}
