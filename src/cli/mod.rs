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
            Commands::Ws(cmd) => {
                if !self.quiet {
                    println!("sv ws {:?} - not yet implemented", cmd);
                }
                Ok(())
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
                    if !self.quiet {
                        println!("sv lease break {:?} {:?} - not yet implemented", ids, reason);
                    }
                    Ok(())
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
            Commands::Risk { .. } => {
                if !self.quiet {
                    println!("sv risk - not yet implemented");
                }
                Ok(())
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
        }
    }
}
