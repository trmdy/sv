//! Command-line interface for sv
//!
//! This module defines the CLI structure using clap derive macros.
//! Each subcommand is defined in its own submodule.

use clap::{Parser, Subcommand};

use crate::error::Result;

mod commit;
mod init;
mod lease;
mod protect;
mod release;
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
    /// Workspace management (worktrees)
    #[command(subcommand)]
    Ws(WsCommands),

    /// Take a lease on paths
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
    Lease(LeaseCommands),

    /// Protected path management
    #[command(subcommand)]
    Protect(ProtectCommands),

    /// Commit with sv checks (protected paths, lease conflicts, Change-Id)
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
    Op(OpCommands),

    /// Undo the last operation
    Undo {
        /// Specific operation ID to undo
        #[arg(long)]
        op: Option<String>,
    },

    /// Set or show actor identity
    #[command(subcommand)]
    Actor(ActorCommands),

    /// Initialize sv in a repository
    Init,

    /// Show current workspace status
    Status,

    /// Hoist workspace branches into an integration branch
    Hoist {
        /// Selector for workspaces to include (e.g., "actor:agent*" or "all")
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
    },
}

/// Workspace subcommands
#[derive(Subcommand, Debug)]
pub enum WsCommands {
    /// Create a new workspace (worktree)
    New {
        /// Workspace name
        name: String,

        /// Base ref to branch from
        #[arg(long)]
        base: Option<String>,

        /// Directory path for the worktree
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
    Here {
        /// Workspace name
        #[arg(long)]
        name: Option<String>,
    },

    /// List workspaces
    List {
        /// Selector to filter workspaces
        #[arg(short, long)]
        selector: Option<String>,
    },

    /// Show detailed workspace info
    Info {
        /// Workspace name
        name: String,
    },

    /// Remove a workspace
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
    Ls {
        /// Selector to filter leases
        #[arg(short, long)]
        selector: Option<String>,

        /// Filter by actor
        #[arg(long)]
        actor: Option<String>,
    },

    /// Show who holds leases on a path
    Who {
        /// Path to check
        path: String,
    },

    /// Renew lease TTL
    Renew {
        /// Lease IDs to renew
        #[arg(required = true)]
        ids: Vec<String>,

        /// New TTL
        #[arg(long)]
        ttl: Option<String>,
    },

    /// Break a lease (emergency override)
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
    Status,

    /// Add protected patterns
    Add {
        /// Patterns to protect
        #[arg(required = true)]
        patterns: Vec<String>,

        /// Protection mode: guard, readonly, warn
        #[arg(long, default_value = "guard")]
        mode: String,
    },

    /// Disable protection for patterns in this workspace
    Off {
        /// Patterns to disable
        #[arg(required = true)]
        patterns: Vec<String>,
    },

    /// Remove protected patterns from config
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
    Log {
        /// Maximum entries to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Filter by actor
        #[arg(long)]
        actor: Option<String>,
    },
}

/// Actor subcommands
#[derive(Subcommand, Debug)]
pub enum ActorCommands {
    /// Set actor identity
    Set {
        /// Actor name
        name: String,
    },

    /// Show current actor
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
    let matching_workspaces: Vec<_> = registry
        .workspaces
        .iter()
        .filter(|ws| {
            if opts.selector == "all" {
                true
            } else if let Some(prefix) = opts.selector.strip_suffix('*') {
                ws.name.starts_with(prefix)
            } else if let Some(actor_prefix) = opts.selector.strip_prefix("actor:") {
                ws.actor.as_ref().map(|a| {
                    if let Some(prefix) = actor_prefix.strip_suffix('*') {
                        a.starts_with(prefix)
                    } else {
                        a == actor_prefix
                    }
                }).unwrap_or(false)
            } else {
                ws.name == opts.selector
            }
        })
        .collect();

    if matching_workspaces.is_empty() {
        return Err(crate::error::Error::InvalidArgument(format!(
            "no workspaces match selector '{}'",
            opts.selector
        )));
    }

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
        commits: Vec::new(), // Will be populated by commit selection task
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
        match self.command {
            Commands::Init => init::run(self.repo, self.json, self.quiet),
            Commands::Status => {
                if !self.quiet {
                    println!("sv status - not yet implemented");
                }
                Ok(())
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
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
            Commands::Release { targets, force } => {
                release::run(release::ReleaseOptions {
                    targets,
                    actor: self.actor,
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
                    if !self.quiet {
                        println!("sv lease renew {:?} {:?} - not yet implemented", ids, ttl);
                    }
                    Ok(())
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
                    if !self.quiet {
                        println!("sv protect off {:?} - not yet implemented", patterns);
                    }
                    Ok(())
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
            Commands::Op(cmd) => {
                if !self.quiet {
                    println!("sv op {:?} - not yet implemented", cmd);
                }
                Ok(())
            }
            Commands::Undo { op } => {
                if !self.quiet {
                    println!("sv undo {:?} - not yet implemented", op);
                }
                Ok(())
            }
            Commands::Actor(cmd) => {
                if !self.quiet {
                    println!("sv actor {:?} - not yet implemented", cmd);
                }
                Ok(())
            }
            Commands::Hoist { selector, dest, strategy, order, dry_run } => {
                run_hoist(HoistOptions {
                    selector,
                    dest,
                    strategy,
                    order,
                    dry_run,
                    repo: self.repo,
                    json: self.json,
                    quiet: self.quiet,
                })
            }
        }
    }
}
