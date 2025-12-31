//! Command-line interface for sv
//!
//! This module defines the CLI structure using clap derive macros.
//! Each subcommand is defined in its own submodule.

use clap::{Parser, Subcommand};

use crate::error::{Error, Result};

mod actor;
mod commit;
mod init;
mod lease;
mod onto;
mod op;
mod protect;
mod release;
mod status;
mod take;
mod ws;

/// sv - Simultaneous Versioning
///
/// A CLI that makes Git practical for many parallel agents by adding
/// workspaces, leases, protected paths, risk prediction, and undo.
#[derive(Parser, Debug)]
#[command(name = "sv")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
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

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Workspace management (workspaces)
    #[command(subcommand)]
    #[command(long_about = r#"Manage workspaces used as agent sandboxes.

Examples:
  sv ws new agent1
  sv ws list
  sv ws info agent1
"#)]
    Ws(WsCommands),

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
    #[command(subcommand)]
    #[command(long_about = r#"Inspect and manage active leases.

Examples:
  sv lease ls
  sv lease who src/auth/token.rs
  sv lease renew 01HZXJ6ZP9QK3A5T --ttl 4h
"#)]
    Lease(LeaseCommands),

    /// Protected path management
    #[command(subcommand)]
    #[command(long_about = r#"Manage protected path rules and overrides.

Examples:
  sv protect status
  sv protect add .beads/** --mode guard
  sv protect off Cargo.lock
"#)]
    Protect(ProtectCommands),

    /// Commit with sv checks (protected paths, lease conflicts, Change-Id)
    #[command(long_about = r#"Commit with sv checks for protected paths and lease conflicts.

Examples:
  sv commit -m "Fix refresh edge case"
  sv commit --amend --no-edit
  sv commit --allow-protected
  sv commit --force-lease
"#)]
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
    #[command(subcommand)]
    #[command(long_about = r#"Inspect operation history.

Examples:
  sv op log --limit 20
  sv op log --actor alice
"#)]
    Op(OpCommands),

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
    #[command(subcommand)]
    #[command(long_about = r#"Manage the actor identity used for leases and ops.

Examples:
  sv actor set alice
  sv actor show
"#)]
    Actor(ActorCommands),

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

    /// Reposition current workspace onto another workspace's branch
    #[command(long_about = r#"Rebase or merge current workspace onto target workspace.

Examples:
  sv onto agent5
  sv onto agent5 --strategy merge
  sv onto agent5 --base main
  sv onto agent5 --preflight
"#)]
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
  sv hoist -s 'ws(active) & ahead("main")' -d main --strategy stack --order workspace
  sv hoist -s "agent*" -d main --dry-run
"#)]
    Hoist {
        /// Selector for workspaces to include (e.g., ws(active) & ahead("main") or legacy actor:agent*)
        #[arg(short, long, required = true)]
        selector: String,

        /// Destination ref to integrate onto (e.g., "main")
        #[arg(short, long, required = true)]
        dest: String,

        /// Integration strategy: stack, rebase, or merge
        #[arg(long, default_value = "stack")]
        strategy: String,

        /// Ordering mode: workspace, time, or explicit
        #[arg(long, default_value = "workspace")]
        order: String,

        /// Dry run: show what would be done without making changes
        #[arg(long)]
        dry_run: bool,

        /// Continue past conflicts, recording them for later resolution
        #[arg(long)]
        continue_on_conflict: bool,
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
    #[command(long_about = r#"List registered workspaces.

Examples:
  sv ws list
  sv ws list -s "agent*"
"#)]
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
    let base_ref = opts
        .base
        .unwrap_or_else(|| config.base.clone());

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
        println!("  {} ({}) - {} files touched", ws.name, ws.branch, ws.files.len());
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
            println!("  {} vs {} - no conflicts", pair.workspace_a, pair.workspace_b);
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
    pub dest: String,
    pub strategy: String,
    pub order: String,
    pub dry_run: bool,
    pub continue_on_conflict: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_on_conflict: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<HoistConflictSummary>,
}

/// Summary of a conflict during hoist
#[derive(Debug, serde::Serialize)]
pub struct HoistConflictSummary {
    pub commit_id: String,
    pub workspace: String,
    pub files: Vec<String>,
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
    use chrono::Utc;
    use uuid::Uuid;
    use crate::git;
    use crate::storage::{Storage, HoistState, HoistStatus};

    // Parse and validate strategy
    let strategy: HoistStrategy = opts.strategy.parse()?;
    let order: HoistOrder = opts.order.parse()?;

    // Open repository
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = repo.path().to_path_buf();
    let storage = Storage::new(workdir.clone(), git_dir, workdir);

    // Validate dest ref exists
    repo.revparse_single(&opts.dest).map_err(|_| {
        crate::error::Error::InvalidArgument(format!(
            "destination ref '{}' does not exist",
            opts.dest
        ))
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
        crate::hoist::select_hoist_commits(&repo, &opts.dest, &workspace_refs, &order_mode)?;
    let hoist_commits = crate::hoist::build_hoist_commits(&repo, &candidates)?;

    // Generate hoist ID and integration branch name
    let hoist_id = Uuid::new_v4().to_string();
    let integration_ref = format!("sv/hoist/{}", opts.dest);

    if opts.dry_run {
        // Dry run output
        let output = HoistOutput {
            hoist_id: hoist_id.clone(),
            dest_ref: opts.dest.clone(),
            integration_ref: integration_ref.clone(),
            strategy,
            order,
            workspaces: matching_workspaces.iter().map(|w| w.name.clone()).collect(),
            status: "dry_run".to_string(),
            continue_on_conflict: if opts.continue_on_conflict { Some(true) } else { None },
            conflicts: Vec::new(),
        };

        if opts.json {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else if !opts.quiet {
            println!("Hoist (dry run)");
            println!("  ID: {}", hoist_id);
            println!("  Dest: {} -> {}", opts.dest, integration_ref);
            println!("  Strategy: {:?}", strategy);
            println!("  Order: {:?}", order);
            println!("  Workspaces: {}", matching_workspaces.len());
            for ws in &matching_workspaces {
                println!("    - {} ({})", ws.name, ws.branch);
            }
        }

        return Ok(());
    }

    // Create or reset integration branch to dest ref
    let dest_commit = repo.revparse_single(&opts.dest)?.peel_to_commit()?;
    
    // Check if integration branch exists
    let branch_exists = repo.find_branch(&integration_ref, git2::BranchType::Local).is_ok();
    
    if branch_exists {
        // Reset existing branch to dest
        let mut branch = repo.find_branch(&integration_ref, git2::BranchType::Local)?;
        branch.get_mut().set_target(dest_commit.id(), &format!("sv hoist: reset to {}", opts.dest))?;
    } else {
        // Create new branch at dest
        repo.branch(&integration_ref, &dest_commit, false)?;
    }

    // Initialize hoist state
    let now = Utc::now();
    let state = HoistState {
        hoist_id: hoist_id.clone(),
        dest_ref: opts.dest.clone(),
        integration_ref: integration_ref.clone(),
        status: HoistStatus::InProgress,
        started_at: now,
        updated_at: now,
        commits: hoist_commits,
    };
    storage.write_hoist_state(&state)?;

    // Output result
    let output = HoistOutput {
        hoist_id: hoist_id.clone(),
        dest_ref: opts.dest.clone(),
        integration_ref: integration_ref.clone(),
        strategy,
        order,
        workspaces: matching_workspaces.iter().map(|w| w.name.clone()).collect(),
        status: "in_progress".to_string(),
        continue_on_conflict: if opts.continue_on_conflict { Some(true) } else { None },
        conflicts: Vec::new(),
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        println!("Hoist initialized");
        println!("  ID: {}", hoist_id);
        println!("  Integration branch: {}", integration_ref);
        println!("  Base: {} ({})", opts.dest, &dest_commit.id().to_string()[..8]);
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
        println!("Next: commits will be selected and replayed (separate task)");
    }

    Ok(())
}

impl Cli {
    /// Execute the CLI command
    pub fn run(self) -> Result<()> {
        let events_to_stdout = matches!(self.events.as_deref(), Some("-"));
        if events_to_stdout && self.json {
            return Err(Error::InvalidArgument(
                "--json requires --events <path> to avoid mixing JSON output with JSONL events"
                    .to_string(),
            ));
        }
        match self.command {
            Commands::Init => init::run(self.repo, self.json, self.quiet),
            Commands::Status => {
                status::run(status::StatusOptions {
                    repo: self.repo,
                    actor: self.actor,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Onto { target, strategy, base, preflight } => {
                onto::run(onto::OntoOptions {
                    target_workspace: target,
                    strategy,
                    base,
                    preflight,
                    actor: self.actor,
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Ws(cmd) => match cmd {
                WsCommands::New { name, base, dir, branch, sparse } => {
                    ws::run_new(ws::NewOptions {
                        name,
                        base,
                        dir,
                        branch,
                        sparse,
                        actor: self.actor,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                WsCommands::Here { name } => {
                    ws::run_here(ws::HereOptions {
                        name,
                        actor: self.actor,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                WsCommands::List { selector } => {
                    ws::run_list(ws::ListOptions {
                        selector,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                WsCommands::Info { name } => {
                    ws::run_info(ws::InfoOptions {
                        name,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                WsCommands::Rm { name, force } => {
                    ws::run_rm(ws::RmOptions {
                        name,
                        force,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
            }
            Commands::Take { paths, strength, intent, scope, ttl, note } => {
                take::run(take::TakeOptions {
                    paths,
                    strength,
                    intent,
                    scope,
                    ttl,
                    note,
                    actor: self.actor,
                    events: self.events.clone(),
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Release { targets, force } => {
                release::run(release::ReleaseOptions {
                    targets,
                    actor: self.actor,
                    events: self.events.clone(),
                    repo: self.repo,
                    force,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Lease(cmd) => match cmd {
                LeaseCommands::Ls { selector, actor } => {
                    lease::run_ls(lease::LsOptions {
                        selector,
                        actor,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                LeaseCommands::Who { path } => {
                    lease::run_who(lease::WhoOptions {
                        path,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                LeaseCommands::Renew { ids, ttl } => {
                    lease::run_renew(lease::RenewOptions {
                        ids,
                        ttl,
                        actor: self.actor,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                LeaseCommands::Break { ids, reason } => {
                    lease::run_break(lease::BreakOptions {
                        ids,
                        reason,
                        actor: self.actor,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
            }
            Commands::Protect(cmd) => match cmd {
                ProtectCommands::Status => {
                    protect::run_status(protect::StatusOptions {
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                ProtectCommands::Add { patterns, mode } => {
                    protect::run_add(protect::AddOptions {
                        patterns,
                        mode,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                ProtectCommands::Off { patterns } => {
                    protect::run_off(protect::OffOptions {
                        patterns,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
                ProtectCommands::Rm { patterns, force } => {
                    protect::run_rm(protect::RmOptions {
                        patterns,
                        force,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
            }
            Commands::Commit { message, file, amend, all, no_edit, allow_protected, force_lease } => {
                commit::run(commit::CommitOptions {
                    message,
                    file,
                    amend,
                    all,
                    no_edit,
                    allow_protected,
                    force_lease,
                    actor: self.actor,
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Risk { selector, base, simulate } => {
                run_risk(RiskOptions {
                    selector,
                    base,
                    simulate,
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Op(cmd) => match cmd {
                OpCommands::Log { limit, actor, operation, since, until } => {
                    op::run_log(op::LogOptions {
                        limit,
                        actor,
                        operation,
                        since,
                        until,
                        repo: self.repo,
                        json: self.json,
                        quiet: self.quiet,
                    })
                }
            },
            Commands::Undo { op } => {
                if !self.quiet {
                    println!("sv undo {:?} - not yet implemented", op);
                }
                Ok(())
            }
            Commands::Actor(cmd) => {
                match cmd {
                    ActorCommands::Set { name } => {
                        actor::run_set(actor::SetOptions {
                            name,
                            repo: self.repo,
                            json: self.json,
                            quiet: self.quiet,
                        })
                    }
                    ActorCommands::Show => {
                        actor::run_show(actor::ShowOptions {
                            repo: self.repo,
                            actor: self.actor,
                            json: self.json,
                            quiet: self.quiet,
                        })
                    }
                }
            }
            Commands::Hoist { selector, dest, strategy, order, dry_run, continue_on_conflict } => {
                run_hoist(HoistOptions {
                    selector,
                    dest,
                    strategy,
                    order,
                    dry_run,
                    continue_on_conflict,
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
        }
    }
}
