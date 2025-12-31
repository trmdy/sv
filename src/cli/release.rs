//! sv release command implementation
//!
//! Releases lease reservations by ID or pathspec.

use std::path::PathBuf;

use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::lease::{Lease, LeaseStatus, LeaseStore};
use crate::lock::{FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::oplog::{LeaseChange, OpLog, OpRecord, UndoData};
use crate::storage::Storage;

/// Options for the release command
pub struct ReleaseOptions {
    pub targets: Vec<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub force: bool,
    pub json: bool,
    pub quiet: bool,
}

/// Result of releasing leases
#[derive(serde::Serialize)]
struct ReleaseReport {
    released: Vec<ReleasedLease>,
    not_found: Vec<String>,
    not_owned: Vec<NotOwnedInfo>,
}

#[derive(Clone, serde::Serialize)]
struct ReleasedLease {
    id: String,
    pathspec: String,
    actor: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct NotOwnedInfo {
    target: String,
    lease_id: String,
    owner: Option<String>,
}

pub fn run(options: ReleaseOptions) -> Result<()> {
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
    
    // Determine current actor
    let current_actor = options.actor
        .or_else(|| storage.read_actor())
        .or_else(|| {
            if config.actor.default != "unknown" {
                Some(config.actor.default.clone())
            } else {
                None
            }
        });
    
    // Acquire lock on leases file
    let leases_file = storage.leases_file();
    let lock_path = leases_file.with_extension("lock");
    let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
    
    // Load existing leases
    let mut leases: Vec<Lease> = storage.read_jsonl(&leases_file)?;
    let mut store = LeaseStore::from_vec(leases.clone());
    
    // Expire stale leases first
    store.expire_stale();
    
    let mut released = Vec::new();
    let mut not_found = Vec::new();
    let mut not_owned = Vec::new();
    
    // Process each target
    for target in &options.targets {
        // Try to parse as UUID first
        if let Ok(uuid) = Uuid::parse_str(target) {
            match find_and_release_by_id(&mut leases, &uuid, current_actor.as_deref(), options.force) {
                ReleaseResult::Released(lease) => {
                    released.push(ReleasedLease {
                        id: lease.id.to_string(),
                        pathspec: lease.pathspec.clone(),
                        actor: lease.actor.clone(),
                    });
                }
                ReleaseResult::NotFound => {
                    not_found.push(target.clone());
                }
                ReleaseResult::NotOwned(lease) => {
                    not_owned.push(NotOwnedInfo {
                        target: target.clone(),
                        lease_id: lease.id.to_string(),
                        owner: lease.actor.clone(),
                    });
                }
            }
        } else {
            // Treat as pathspec - release all matching leases owned by current actor
            let matching = find_and_release_by_pathspec(
                &mut leases,
                target,
                current_actor.as_deref(),
                options.force,
            );
            
            if matching.is_empty() {
                not_found.push(target.clone());
            } else {
                for result in matching {
                    match result {
                        ReleaseResult::Released(lease) => {
                            released.push(ReleasedLease {
                                id: lease.id.to_string(),
                                pathspec: lease.pathspec.clone(),
                                actor: lease.actor.clone(),
                            });
                        }
                        ReleaseResult::NotOwned(lease) => {
                            not_owned.push(NotOwnedInfo {
                                target: target.clone(),
                                lease_id: lease.id.to_string(),
                                owner: lease.actor.clone(),
                            });
                        }
                        ReleaseResult::NotFound => {}
                    }
                }
            }
        }
    }
    
    // Write updated leases back
    if !released.is_empty() {
        write_leases(&leases_file, &leases)?;
        
        // Record operation in oplog for undo support
        let oplog = OpLog::for_storage(&storage);
        let pathspecs: Vec<_> = released.iter().map(|l| l.pathspec.clone()).collect();
        let mut record = OpRecord::new(
            format!("sv release {}", pathspecs.join(" ")),
            current_actor.clone(),
        );
        record.undo_data = Some(UndoData {
            lease_changes: released
                .iter()
                .map(|l| LeaseChange {
                    lease_id: l.id.clone(),
                    action: "release".to_string(),
                })
                .collect(),
            ..UndoData::default()
        });
        // Best effort - don't fail the command if oplog write fails
        let _ = oplog.append(&record);
    }
    
    // Output results
    let report = ReleaseReport {
        released: released.clone(),
        not_found: not_found.clone(),
        not_owned: not_owned.clone(),
    };
    
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if !released.is_empty() {
            println!("Released {} lease(s):", released.len());
            for lease in &released {
                let short_id = lease.id.split('-').next().unwrap_or(&lease.id);
                println!("  {} {}", short_id, lease.pathspec);
            }
        }
        
        if !not_found.is_empty() {
            eprintln!("\nNot found ({}):", not_found.len());
            for target in &not_found {
                eprintln!("  {}", target);
            }
        }
        
        if !not_owned.is_empty() {
            eprintln!("\nNot owned by you ({}):", not_owned.len());
            for info in &not_owned {
                eprintln!(
                    "  {} owned by {} (use --force to override)",
                    info.target,
                    info.owner.as_deref().unwrap_or("(ownerless)")
                );
            }
        }
        
        if released.is_empty() && not_found.is_empty() && not_owned.is_empty() {
            println!("No leases matched.");
        }
    }
    
    // Return error if nothing was released and there were targets
    if released.is_empty() && (!not_found.is_empty() || !not_owned.is_empty()) {
        if !not_owned.is_empty() {
            return Err(Error::OperationFailed(format!(
                "Cannot release lease owned by another actor. Use --force to override."
            )));
        }
        return Err(Error::OperationFailed("No matching leases found.".to_string()));
    }
    
    Ok(())
}

enum ReleaseResult {
    Released(Lease),
    NotFound,
    NotOwned(Lease),
}

fn find_and_release_by_id(
    leases: &mut [Lease],
    id: &Uuid,
    current_actor: Option<&str>,
    force: bool,
) -> ReleaseResult {
    for lease in leases.iter_mut() {
        if lease.id == *id && lease.status == LeaseStatus::Active {
            // Check ownership
            if !force {
                if let Some(ref owner) = lease.actor {
                    if let Some(actor) = current_actor {
                        if owner != actor {
                            return ReleaseResult::NotOwned(lease.clone());
                        }
                    }
                }
            }
            
            lease.release();
            return ReleaseResult::Released(lease.clone());
        }
    }
    
    ReleaseResult::NotFound
}

fn find_and_release_by_pathspec(
    leases: &mut [Lease],
    pathspec: &str,
    current_actor: Option<&str>,
    force: bool,
) -> Vec<ReleaseResult> {
    let mut results = Vec::new();
    
    for lease in leases.iter_mut() {
        if lease.status != LeaseStatus::Active {
            continue;
        }
        
        // Check if pathspec matches
        let matches = lease.pathspec == pathspec
            || lease.matches_path(pathspec)
            || lease.pathspec_overlaps(pathspec);
        
        if !matches {
            continue;
        }
        
        // Check ownership unless force
        if !force {
            if let Some(ref owner) = lease.actor {
                if let Some(actor) = current_actor {
                    if owner != actor {
                        results.push(ReleaseResult::NotOwned(lease.clone()));
                        continue;
                    }
                }
            }
            
            // If current actor is None, only release ownerless leases
            if current_actor.is_none() && lease.actor.is_some() {
                results.push(ReleaseResult::NotOwned(lease.clone()));
                continue;
            }
        }
        
        lease.release();
        results.push(ReleaseResult::Released(lease.clone()));
    }
    
    results
}

fn write_leases(path: &PathBuf, leases: &[Lease]) -> Result<()> {
    use std::fs::File;
    use std::io::Write;
    
    // Write to temp file first
    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path)?;
    
    for lease in leases {
        let json = serde_json::to_string(lease)?;
        writeln!(file, "{}", json)?;
    }
    
    file.sync_all()?;
    
    // Atomic rename
    std::fs::rename(&temp_path, path)?;
    
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
