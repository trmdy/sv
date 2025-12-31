//! sv take command implementation
//!
//! Creates lease reservations on paths.

use std::path::PathBuf;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::lease::{parse_duration, Lease, LeaseIntent, LeaseScope, LeaseStore, LeaseStrength};
use crate::storage::Storage;

/// Options for the take command
pub struct TakeOptions {
    pub paths: Vec<String>,
    pub strength: String,
    pub intent: String,
    pub scope: String,
    pub ttl: String,
    pub note: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of taking leases
#[derive(serde::Serialize)]
struct TakeReport {
    leases: Vec<LeaseInfo>,
    conflicts: Vec<ConflictInfo>,
}

#[derive(serde::Serialize)]
struct LeaseInfo {
    id: String,
    pathspec: String,
    strength: String,
    intent: String,
    actor: Option<String>,
    expires_at: String,
}

#[derive(Clone, serde::Serialize)]
struct ConflictInfo {
    pathspec: String,
    conflicting_lease_id: String,
    conflicting_actor: Option<String>,
    conflicting_strength: String,
}

pub fn run(options: TakeOptions) -> Result<()> {
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
    
    // Ensure sv is initialized
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string()
        ));
    }
    
    // Load config
    let config = Config::load_from_repo(&workdir);
    
    // Parse strength
    let strength: LeaseStrength = options.strength.parse()?;
    
    // Parse intent
    let intent: LeaseIntent = options.intent.parse()?;
    
    // Parse scope
    let scope: LeaseScope = options.scope.parse()?;
    
    // Determine actor
    let actor = options.actor
        .or_else(|| storage.read_actor())
        .or_else(|| {
            if config.actor.default != "unknown" {
                Some(config.actor.default.clone())
            } else {
                None
            }
        });
    
    // Load existing leases
    let existing_leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(existing_leases);
    
    // Expire stale leases
    store.expire_stale();

    // Cleanup expired leases with configured grace period
    let grace = parse_duration(&config.leases.expiration_grace)?;
    let _expired = store.cleanup_expired(grace);
    
    // Check note requirement
    if strength.requires_note() && options.note.is_none() {
        return Err(Error::NoteRequired(strength.to_string()));
    }
    
    let mut created_leases = Vec::new();
    let mut conflicts = Vec::new();
    
    // Create leases for each path
    for pathspec in &options.paths {
        // Check for conflicts
        let path_conflicts = store.check_conflicts(
            pathspec,
            strength,
            actor.as_deref(),
            false, // TODO: support --allow-overlap flag
        );
        
        if !path_conflicts.is_empty() {
            for conflict in path_conflicts {
                conflicts.push(ConflictInfo {
                    pathspec: pathspec.clone(),
                    conflicting_lease_id: conflict.id.to_string(),
                    conflicting_actor: conflict.actor.clone(),
                    conflicting_strength: conflict.strength.to_string(),
                });
            }
            continue;
        }
        
        // Build the lease
        let mut builder = Lease::builder(pathspec)
            .strength(strength)
            .intent(intent)
            .scope(scope.clone())
            .ttl(&options.ttl);
        
        if let Some(ref actor_name) = actor {
            builder = builder.actor(actor_name);
        }
        
        if let Some(ref note) = options.note {
            builder = builder.note(note);
        }
        
        let lease = builder.build()?;
        
        // Add to store for conflict checking of subsequent paths
        store.add(lease.clone());
        
        created_leases.push(lease);
    }
    
    // Write all new leases to storage
    for lease in &created_leases {
        storage.append_jsonl(&storage.leases_file(), lease)?;
    }
    
    // Output results
    let report = TakeReport {
        leases: created_leases.iter().map(|l| LeaseInfo {
            id: l.id.to_string(),
            pathspec: l.pathspec.clone(),
            strength: l.strength.to_string(),
            intent: l.intent.to_string(),
            actor: l.actor.clone(),
            expires_at: l.expires_at.to_rfc3339(),
        }).collect(),
        conflicts: conflicts.clone(),
    };
    
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if !created_leases.is_empty() {
            println!("Created {} lease(s):", created_leases.len());
            for lease in &created_leases {
                println!(
                    "  {} {} [{}] (expires {})",
                    lease.id.to_string().split('-').next().unwrap_or(""),
                    lease.pathspec,
                    lease.strength,
                    format_relative_time(&lease.expires_at),
                );
            }
        }
        
        if !conflicts.is_empty() {
            eprintln!("\nConflicts ({}):", conflicts.len());
            for conflict in &conflicts {
                eprintln!(
                    "  {} conflicts with {} lease by {}",
                    conflict.pathspec,
                    conflict.conflicting_strength,
                    conflict.conflicting_actor.as_deref().unwrap_or("(ownerless)"),
                );
            }
        }
        
        if created_leases.is_empty() && conflicts.is_empty() {
            println!("No leases created.");
        }
    }
    
    // Return error if there were conflicts and no leases created
    if created_leases.is_empty() && !conflicts.is_empty() {
        return Err(Error::LeaseConflict {
            path: conflicts[0].pathspec.clone().into(),
            holder: conflicts[0].conflicting_actor.clone().unwrap_or_else(|| "(ownerless)".to_string()),
            strength: conflicts[0].conflicting_strength.clone(),
        });
    }
    
    Ok(())
}

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
