//! sv take command implementation
//!
//! Creates lease reservations on paths.

use std::path::PathBuf;

use crate::actor;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::events::{Event, EventDestination, EventKind};
use crate::lease::{parse_duration, Lease, LeaseIntent, LeaseScope, LeaseStore, LeaseStrength};
use crate::lock::{FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::oplog::{LeaseChange, OpLog, OpRecord, UndoData};
use crate::output::{emit_success, HumanOutput, OutputOptions};
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
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of taking leases
#[derive(serde::Serialize)]
struct TakeReport {
    actor: String,
    created: Vec<LeaseInfo>,
    updated: Vec<LeaseInfo>,
    conflicts: Vec<ConflictInfo>,
    summary: TakeSummary,
}

#[derive(serde::Serialize)]
struct TakeSummary {
    created: usize,
    updated: usize,
    conflicts: usize,
}

#[derive(serde::Serialize)]
struct LeaseInfo {
    id: String,
    path: String,
    strength: String,
    intent: String,
    actor: Option<String>,
    ttl: String,
    expires_at: String,
}

#[derive(Clone, serde::Serialize)]
struct ConflictInfo {
    path: String,
    holder: Option<String>,
    strength: String,
    lease_id: String,
}

#[derive(serde::Serialize)]
struct LeaseEventData {
    id: String,
    pathspec: String,
    strength: String,
    intent: String,
    scope: String,
    actor: Option<String>,
    ttl: String,
    expires_at: String,
    created_at: String,
    note: Option<String>,
}

pub fn run(options: TakeOptions) -> Result<()> {
    // Discover repository
    let start = options
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let repository =
        git2::Repository::discover(&start).map_err(|_| Error::RepoNotFound(start.clone()))?;

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
            "sv not initialized. Run 'sv init' first.".to_string(),
        ));
    }

    // Load config
    let config = Config::load_from_repo(&workdir);

    let event_destination = EventDestination::parse(options.events.as_deref());
    let mut event_sink = event_destination
        .as_ref()
        .map(|dest| dest.open())
        .transpose()?;

    // Parse strength
    let strength: LeaseStrength = options.strength.parse()?;

    // Parse intent
    let intent: LeaseIntent = options.intent.parse()?;

    // Parse scope
    let scope: LeaseScope = options.scope.parse()?;

    // Determine actor (CLI override, env, persisted, config)
    let actor = actor::resolve_actor_optional(Some(&workdir), options.actor.as_deref())?;

    // Lock leases file to prevent concurrent writers from corrupting JSONL.
    let leases_file = storage.leases_file();
    let lock_path = leases_file.with_extension("lock");
    let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

    // Load existing leases
    let existing_leases: Vec<Lease> = storage.read_jsonl(&leases_file)?;
    let mut store = LeaseStore::from_vec(existing_leases);

    // Expire stale leases
    store.expire_stale();

    // Cleanup expired leases with configured grace period
    let grace = parse_duration(&config.leases.expiration_grace)?;
    let _expired = store.cleanup_expired(grace);

    // Check note requirement
    if config.leases.require_note && strength.requires_note() && options.note.is_none() {
        return Err(Error::NoteRequired(strength.to_string()));
    }

    let mut created_leases = Vec::new();
    let mut updated_leases = Vec::new();
    let mut conflicts = Vec::new();

    // Create or update leases for each path
    for pathspec in &options.paths {
        // Check for conflicts with OTHER actors
        let path_conflicts = store.check_conflicts(
            pathspec,
            strength,
            actor.as_deref(),
            false, // TODO: support --allow-overlap flag
        );

        if !path_conflicts.is_empty() {
            for conflict in path_conflicts {
                conflicts.push(ConflictInfo {
                    path: pathspec.clone(),
                    holder: conflict.actor.clone(),
                    strength: conflict.strength.to_string(),
                    lease_id: conflict.id.to_string(),
                });
            }
            continue;
        }

        // Check if this actor already has a lease on this exact path (upsert)
        if let Some(actor_name) = actor.as_deref() {
            if let Some(existing) = store.find_by_actor_and_path_mut(actor_name, pathspec) {
                // Update existing lease instead of creating new one
                existing.update(
                    strength,
                    intent,
                    scope.clone(),
                    &options.ttl,
                    options.note.clone(),
                )?;
                updated_leases.push(existing.clone());
                continue;
            }
        }

        // Build a new lease
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

    // Write leases to storage
    if !updated_leases.is_empty() {
        // If we updated any leases, we need to rewrite the entire file
        storage.save_leases(&store)?;
    } else {
        // Only new leases - can just append
        for lease in &created_leases {
            storage.append_jsonl(&leases_file, lease)?;
        }
    }

    // Record operation in oplog for undo support
    if !created_leases.is_empty() || !updated_leases.is_empty() {
        let oplog = OpLog::for_storage(&storage);
        let all_pathspecs: Vec<_> = created_leases
            .iter()
            .chain(updated_leases.iter())
            .map(|l| l.pathspec.clone())
            .collect();
        let mut record = OpRecord::new(
            format!("sv take {}", all_pathspecs.join(" ")),
            actor.clone(),
        );

        let mut lease_changes: Vec<LeaseChange> = created_leases
            .iter()
            .map(|l| LeaseChange {
                lease_id: l.id.to_string(),
                action: "create".to_string(),
            })
            .collect();

        lease_changes.extend(updated_leases.iter().map(|l| LeaseChange {
            lease_id: l.id.to_string(),
            action: "update".to_string(),
        }));

        record.undo_data = Some(UndoData {
            lease_changes,
            ..UndoData::default()
        });
        // Best effort - don't fail the command if oplog write fails
        let _ = oplog.append(&record);
    }

    let mut event_warning: Option<String> = None;
    if let Some(sink) = event_sink.as_mut() {
        // Emit events for created leases
        for lease in &created_leases {
            let event = match Event::new(EventKind::LeaseCreated, lease.actor.clone()).with_data(
                LeaseEventData {
                    id: lease.id.to_string(),
                    pathspec: lease.pathspec.clone(),
                    strength: lease.strength.to_string(),
                    intent: lease.intent.to_string(),
                    scope: lease.scope.to_string(),
                    actor: lease.actor.clone(),
                    ttl: lease.ttl.clone(),
                    expires_at: lease.expires_at.to_rfc3339(),
                    created_at: lease.created_at.to_rfc3339(),
                    note: lease.note.clone(),
                },
            ) {
                Ok(event) => event,
                Err(err) => {
                    event_warning = Some(format!("event output failed: {err}"));
                    break;
                }
            };
            if let Err(err) = sink.emit(&event) {
                event_warning = Some(format!("event output failed: {err}"));
                break;
            }
        }
        // Emit events for updated leases (using LeaseCreated for now, could add LeaseUpdated)
        for lease in &updated_leases {
            let event = match Event::new(EventKind::LeaseCreated, lease.actor.clone()).with_data(
                LeaseEventData {
                    id: lease.id.to_string(),
                    pathspec: lease.pathspec.clone(),
                    strength: lease.strength.to_string(),
                    intent: lease.intent.to_string(),
                    scope: lease.scope.to_string(),
                    actor: lease.actor.clone(),
                    ttl: lease.ttl.clone(),
                    expires_at: lease.expires_at.to_rfc3339(),
                    created_at: lease.created_at.to_rfc3339(),
                    note: lease.note.clone(),
                },
            ) {
                Ok(event) => event,
                Err(err) => {
                    event_warning = Some(format!("event output failed: {err}"));
                    break;
                }
            };
            if let Err(err) = sink.emit(&event) {
                event_warning = Some(format!("event output failed: {err}"));
                break;
            }
        }
    }

    // Output results
    let actor_label = actor.clone().unwrap_or_else(|| "unknown".to_string());
    let lease_to_info = |l: &Lease| LeaseInfo {
        id: l.id.to_string(),
        path: l.pathspec.clone(),
        strength: l.strength.to_string(),
        intent: l.intent.to_string(),
        actor: l.actor.clone(),
        ttl: l.ttl.clone(),
        expires_at: l.expires_at.to_rfc3339(),
    };

    let report = TakeReport {
        actor: actor_label.clone(),
        created: created_leases.iter().map(lease_to_info).collect(),
        updated: updated_leases.iter().map(lease_to_info).collect(),
        conflicts: conflicts.clone(),
        summary: TakeSummary {
            created: created_leases.len(),
            updated: updated_leases.len(),
            conflicts: conflicts.len(),
        },
    };

    let events_to_stdout = matches!(event_destination, Some(EventDestination::Stdout));
    let total_leases = created_leases.len() + updated_leases.len();
    let header = if total_leases > 0 && !conflicts.is_empty() {
        if updated_leases.is_empty() {
            format!(
                "sv take: created {} lease(s) ({} conflict(s))",
                created_leases.len(),
                conflicts.len()
            )
        } else if created_leases.is_empty() {
            format!(
                "sv take: updated {} lease(s) ({} conflict(s))",
                updated_leases.len(),
                conflicts.len()
            )
        } else {
            format!(
                "sv take: created {}, updated {} lease(s) ({} conflict(s))",
                created_leases.len(),
                updated_leases.len(),
                conflicts.len()
            )
        }
    } else if total_leases > 0 {
        if updated_leases.is_empty() {
            format!("sv take: created {} lease(s)", created_leases.len())
        } else if created_leases.is_empty() {
            format!("sv take: updated {} lease(s)", updated_leases.len())
        } else {
            format!(
                "sv take: created {}, updated {} lease(s)",
                created_leases.len(),
                updated_leases.len()
            )
        }
    } else if !conflicts.is_empty() {
        format!("sv take: {} conflict(s)", conflicts.len())
    } else {
        "sv take: no leases created".to_string()
    };

    let mut human = HumanOutput::new(header);
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("actor", actor_label);
    human.push_summary("leases_created", created_leases.len().to_string());
    human.push_summary("leases_updated", updated_leases.len().to_string());
    human.push_summary("conflicts", conflicts.len().to_string());

    for lease in &created_leases {
        human.push_detail(format!(
            "{} ({}, intent: {}, ttl: {}, expires {})",
            lease.pathspec,
            lease.strength,
            lease.intent,
            lease.ttl,
            format_relative_time(&lease.expires_at)
        ));
    }

    for lease in &updated_leases {
        human.push_detail(format!(
            "{} (updated: {}, intent: {}, ttl: {}, expires {})",
            lease.pathspec,
            lease.strength,
            lease.intent,
            lease.ttl,
            format_relative_time(&lease.expires_at)
        ));
    }

    for conflict in &conflicts {
        human.push_warning(format!(
            "conflict: {} held by {} ({})",
            conflict.path,
            conflict.holder.as_deref().unwrap_or("(ownerless)"),
            conflict.strength
        ));
    }

    if let Some(conflict) = conflicts.first() {
        human.push_next_step(format!("sv lease who {}", conflict.path));
        human.push_next_step("retry with --allow-overlap if intentional");
    }

    let conflicts_only =
        created_leases.is_empty() && updated_leases.is_empty() && !conflicts.is_empty();
    if !conflicts_only {
        emit_success(
            OutputOptions {
                json: options.json && !events_to_stdout,
                quiet: options.quiet || events_to_stdout,
            },
            "take",
            &report,
            Some(&human),
        )?;
    }

    // Return error if there were conflicts and no leases created or updated
    if conflicts_only {
        return Err(Error::LeaseConflict {
            path: conflicts[0].path.clone().into(),
            holder: conflicts[0]
                .holder
                .clone()
                .unwrap_or_else(|| "(ownerless)".to_string()),
            strength: conflicts[0].strength.clone(),
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
