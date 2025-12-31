//! sv lease subcommand implementations
//!
//! Provides lease management commands: ls, who, renew, break

use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::lease::{parse_duration, Lease, LeaseStatus, LeaseStore};
use crate::oplog::{OpLog, OpOutcome, OpRecord, UndoData};
use crate::storage::Storage;

/// Options for the lease ls command
pub struct LsOptions {
    pub selector: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Lease entry for display/JSON output
#[derive(serde::Serialize)]
struct LeaseEntry {
    id: String,
    pathspec: String,
    strength: String,
    intent: String,
    actor: Option<String>,
    scope: String,
    expires_at: String,
    note: Option<String>,
    status: String,
    created_at: String,
}

/// Result of lease ls command
#[derive(serde::Serialize)]
struct LsReport {
    leases: Vec<LeaseEntry>,
    total: usize,
    active: usize,
}

/// Run the lease ls command
pub fn run_ls(options: LsOptions) -> Result<()> {
    // Discover repository
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    
    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;
    
    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?
        .to_path_buf();
    
    // Resolve common dir for worktree support
    let common_dir = resolve_common_dir(&repository)?;
    
    // Initialize storage
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());
    
    // Check if sv is initialized
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string()
        ));
    }
    
    // Load config
    let config = Config::load_from_repo(&workdir);

    // Load existing leases
    let existing_leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(existing_leases);
    
    // Expire stale leases (updates status but keeps them in store)
    store.expire_stale();
    let grace = parse_duration(&config.leases.expiration_grace)?;
    let _expired = store.cleanup_expired(grace);
    
    // Get all leases and filter
    let total = store.all().len();
    let mut leases: Vec<&Lease> = store.active().collect();
    
    // Filter by actor if specified
    if let Some(ref actor_filter) = options.actor {
        leases.retain(|l| {
            l.actor.as_ref().map(|a| a == actor_filter).unwrap_or(false)
        });
    }
    
    // TODO: Apply selector filter when selector language is implemented
    if options.selector.is_some() {
        // For now, selector is a stub - just log it
        if !options.quiet && !options.json {
            eprintln!("Note: selector filtering not yet implemented");
        }
    }
    
    let active_count = leases.len();
    
    // Convert to display format
    let entries: Vec<LeaseEntry> = leases
        .iter()
        .map(|l| LeaseEntry {
            id: l.id.to_string(),
            pathspec: l.pathspec.clone(),
            strength: l.strength.to_string(),
            intent: l.intent.to_string(),
            actor: l.actor.clone(),
            scope: l.scope.to_string(),
            expires_at: l.expires_at.to_rfc3339(),
            note: l.note.clone(),
            status: format_status(&l.status),
            created_at: l.created_at.to_rfc3339(),
        })
        .collect();
    
    // Output results
    let report = LsReport {
        leases: entries,
        total,
        active: active_count,
    };
    
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if report.leases.is_empty() {
            println!("No active leases.");
        } else {
            println!("Active leases ({}):", report.active);
            println!();
            for lease in &report.leases {
                print_lease(lease);
            }
        }
    }
    
    Ok(())
}

/// Options for the lease who command
pub struct WhoOptions {
    pub path: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of lease who command
#[derive(serde::Serialize)]
struct WhoReport {
    path: String,
    leases: Vec<LeaseEntry>,
}

/// Run the lease who command
pub fn run_who(options: WhoOptions) -> Result<()> {
    // Discover repository
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    
    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;
    
    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?
        .to_path_buf();
    
    // Resolve common dir for worktree support
    let common_dir = resolve_common_dir(&repository)?;
    
    // Initialize storage
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());
    
    // Check if sv is initialized
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string()
        ));
    }
    
    // Load config
    let config = Config::load_from_repo(&workdir);

    // Load existing leases
    let existing_leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(existing_leases);
    
    // Expire stale leases
    store.expire_stale();
    let grace = parse_duration(&config.leases.expiration_grace)?;
    let _expired = store.cleanup_expired(grace);
    
    // Find leases overlapping with the given path
    let leases: Vec<&Lease> = store.overlapping_path(&options.path).collect();
    
    // Convert to display format
    let entries: Vec<LeaseEntry> = leases
        .iter()
        .map(|l| LeaseEntry {
            id: l.id.to_string(),
            pathspec: l.pathspec.clone(),
            strength: l.strength.to_string(),
            intent: l.intent.to_string(),
            actor: l.actor.clone(),
            scope: l.scope.to_string(),
            expires_at: l.expires_at.to_rfc3339(),
            note: l.note.clone(),
            status: format_status(&l.status),
            created_at: l.created_at.to_rfc3339(),
        })
        .collect();
    
    // Output results
    let report = WhoReport {
        path: options.path.clone(),
        leases: entries,
    };
    
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if report.leases.is_empty() {
            println!("No active leases on '{}'", options.path);
        } else {
            println!("Leases on '{}' ({}):", options.path, report.leases.len());
            println!();
            for lease in &report.leases {
                print_lease(lease);
            }
        }
    }
    
    Ok(())
}

// =============================================================================
// Helper functions
// =============================================================================

fn resolve_common_dir(repository: &git2::Repository) -> Result<PathBuf> {
    let git_dir = repository.path();
    let commondir_path = git_dir.join("commondir");
    if !commondir_path.exists() {
        return Ok(git_dir.to_path_buf());
    }

    let content = std::fs::read_to_string(&commondir_path)?;
    let rel = content.trim();
    if rel.is_empty() {
        return Err(Error::OperationFailed(format!(
            "commondir file is empty: {}",
            commondir_path.display()
        )));
    }

    Ok(git_dir.join(rel))
}

fn format_status(status: &LeaseStatus) -> String {
    match status {
        LeaseStatus::Active => "active".to_string(),
        LeaseStatus::Released => "released".to_string(),
        LeaseStatus::Expired => "expired".to_string(),
        LeaseStatus::Broken => "broken".to_string(),
    }
}

fn format_relative_time(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = *dt - now;
    
    if diff.num_hours() > 0 {
        format!("in {}h {}m", diff.num_hours(), diff.num_minutes() % 60)
    } else if diff.num_minutes() > 0 {
        format!("in {}m", diff.num_minutes())
    } else if diff.num_seconds() > 0 {
        format!("in {}s", diff.num_seconds())
    } else {
        "expired".to_string()
    }
}

fn print_lease(lease: &LeaseEntry) {
    // Parse the expiry for relative time display
    let expires_relative = chrono::DateTime::parse_from_rfc3339(&lease.expires_at)
        .map(|dt| format_relative_time(&dt.with_timezone(&chrono::Utc)))
        .unwrap_or_else(|_| lease.expires_at.clone());
    
    // Short ID (first segment of UUID)
    let short_id = lease.id.split('-').next().unwrap_or(&lease.id);
    
    // Actor display
    let actor_display = lease.actor.as_deref().unwrap_or("(ownerless)");
    
    println!(
        "  {} {} [{}] by {} (expires {})",
        short_id,
        lease.pathspec,
        lease.strength,
        actor_display,
        expires_relative,
    );
    
    // Show note if present (indented)
    if let Some(ref note) = lease.note {
        println!("       └─ {}", note);
    }
}
