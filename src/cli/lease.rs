//! sv lease subcommand implementations
//!
//! Provides lease management commands: ls, who, renew, break, wait

use std::path::PathBuf;
use std::time::{Duration as StdDuration, Instant};

// chrono::Utc is used via Lease methods
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::lease::{parse_duration, Lease, LeaseStatus, LeaseStore};
use crate::lock::{FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::oplog::{LeaseChange, OpLog, OpOutcome, OpRecord, UndoData};
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

    // Check if sv is initialized
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
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
        leases.retain(|l| l.actor.as_ref().map(|a| a == actor_filter).unwrap_or(false));
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

    // Check if sv is initialized
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
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
        short_id, lease.pathspec, lease.strength, actor_display, expires_relative,
    );

    // Show note if present (indented)
    if let Some(ref note) = lease.note {
        println!("       └─ {}", note);
    }
}

// =============================================================================
// sv lease renew
// =============================================================================

/// Options for the lease renew command
pub struct RenewOptions {
    pub ids: Vec<String>,
    pub ttl: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of renewing a lease
#[derive(Clone, serde::Serialize)]
struct RenewedLeaseInfo {
    id: String,
    pathspec: String,
    actor: Option<String>,
    ttl: String,
    expires_at: String,
}

#[derive(Clone, serde::Serialize)]
struct NotOwnedInfo {
    target: String,
    lease_id: String,
    owner: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct NotActiveInfo {
    target: String,
    lease_id: String,
    status: String,
}

/// Report for lease renew command
#[derive(serde::Serialize)]
struct RenewReport {
    renewed: Vec<RenewedLeaseInfo>,
    not_found: Vec<String>,
    not_owned: Vec<NotOwnedInfo>,
    not_active: Vec<NotActiveInfo>,
}

/// Run the lease renew command
pub fn run_renew(options: RenewOptions) -> Result<()> {
    if let Some(ttl) = options.ttl.as_deref() {
        if ttl.trim().is_empty() {
            return Err(Error::InvalidArgument("--ttl cannot be empty".to_string()));
        }
    }

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

    let common_dir = resolve_common_dir(&repository)?;
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());

    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
        ));
    }

    let config = Config::load_from_repo(&workdir);
    let current_actor = options.actor.or_else(|| storage.read_actor()).or_else(|| {
        if config.actor.default != "unknown" {
            Some(config.actor.default.clone())
        } else {
            None
        }
    });

    let leases_file = storage.leases_file();
    let lock_path = leases_file.with_extension("lock");
    let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

    let mut store = LeaseStore::from_vec(storage.read_jsonl(&leases_file)?);
    store.expire_stale();
    let mut leases = store.into_vec();

    let mut renewed = Vec::new();
    let mut not_found = Vec::new();
    let mut not_owned = Vec::new();
    let mut not_active = Vec::new();

    for target in &options.ids {
        let idx = match find_lease_index(&leases, target) {
            Some(idx) => idx,
            None => {
                not_found.push(target.clone());
                continue;
            }
        };

        let lease = &mut leases[idx];
        if lease.status != LeaseStatus::Active {
            not_active.push(NotActiveInfo {
                target: target.clone(),
                lease_id: lease.id.to_string(),
                status: lease_status_label(&lease.status),
            });
            continue;
        }

        if let Some(owner) = lease.actor.as_deref() {
            match current_actor.as_deref() {
                Some(actor) if actor == owner => {}
                Some(_) | None => {
                    not_owned.push(NotOwnedInfo {
                        target: target.clone(),
                        lease_id: lease.id.to_string(),
                        owner: lease.actor.clone(),
                    });
                    continue;
                }
            }
        }

        let ttl = options.ttl.clone().unwrap_or_else(|| {
            if lease.ttl.trim().is_empty() {
                config.leases.default_ttl.clone()
            } else {
                lease.ttl.clone()
            }
        });

        lease.renew(ttl.clone())?;
        renewed.push(RenewedLeaseInfo {
            id: lease.id.to_string(),
            pathspec: lease.pathspec.clone(),
            actor: lease.actor.clone(),
            ttl: lease.ttl.clone(),
            expires_at: lease.expires_at.to_rfc3339(),
        });
    }

    if !renewed.is_empty() {
        write_leases_jsonl(&storage.leases_file(), &leases)?;

        let oplog = OpLog::for_storage(&storage);
        let mut record = OpRecord::new(
            format!("sv lease renew {}", options.ids.join(" ")),
            current_actor.clone(),
        );
        record.outcome = OpOutcome::success();
        let _ = oplog.append(&record);
    }

    let report = RenewReport {
        renewed: renewed.clone(),
        not_found: not_found.clone(),
        not_owned: not_owned.clone(),
        not_active: not_active.clone(),
    };

    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if !renewed.is_empty() {
            println!("Renewed {} lease(s):", renewed.len());
            for info in &renewed {
                let short_id = info.id.split('-').next().unwrap_or(&info.id);
                let actor_display = info.actor.as_deref().unwrap_or("(ownerless)");
                println!(
                    "  {} {} by {} (expires {})",
                    short_id, info.pathspec, actor_display, info.expires_at
                );
            }
        }
        if !not_owned.is_empty() {
            println!("\nNot owned ({}):", not_owned.len());
            for info in &not_owned {
                let short_id = info.lease_id.split('-').next().unwrap_or(&info.lease_id);
                let owner_display = info.owner.as_deref().unwrap_or("(ownerless)");
                println!("  {} (owner: {})", short_id, owner_display);
            }
        }
        if !not_active.is_empty() {
            println!("\nNot active ({}):", not_active.len());
            for info in &not_active {
                let short_id = info.lease_id.split('-').next().unwrap_or(&info.lease_id);
                println!("  {} (status: {})", short_id, info.status);
            }
        }
        if !not_found.is_empty() {
            println!("\nNot found ({}):", not_found.len());
            for id in &not_found {
                println!("  {}", id);
            }
        }
    }

    if renewed.is_empty() && !not_found.is_empty() && not_owned.is_empty() && not_active.is_empty()
    {
        return Err(Error::LeaseNotFound(not_found.join(", ")));
    }

    Ok(())
}

fn find_lease_index(leases: &[Lease], id_str: &str) -> Option<usize> {
    if let Ok(uuid) = Uuid::parse_str(id_str) {
        return leases.iter().position(|lease| lease.id == uuid);
    }

    let normalized = id_str.to_lowercase();
    leases
        .iter()
        .position(|lease| lease.id.to_string().to_lowercase().starts_with(&normalized))
}

fn lease_status_label(status: &LeaseStatus) -> String {
    match status {
        LeaseStatus::Active => "active".to_string(),
        LeaseStatus::Released => "released".to_string(),
        LeaseStatus::Expired => "expired".to_string(),
        LeaseStatus::Broken => "broken".to_string(),
    }
}

// =============================================================================
// sv lease break
// =============================================================================

/// Options for the lease break command
pub struct BreakOptions {
    pub ids: Vec<String>,
    pub reason: String,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of breaking a lease
#[derive(Clone, serde::Serialize)]
struct BrokenLeaseInfo {
    id: String,
    pathspec: String,
    actor: Option<String>,
    strength: String,
    reason: String,
}

/// Report for lease break command
#[derive(serde::Serialize)]
struct BreakReport {
    broken: Vec<BrokenLeaseInfo>,
    not_found: Vec<String>,
}

/// Run the lease break command
///
/// Force-releases leases regardless of ownership, with mandatory audit trail.
pub fn run_break(options: BreakOptions) -> Result<()> {
    // Validate reason is provided and not empty
    if options.reason.trim().is_empty() {
        return Err(Error::InvalidArgument(
            "--reason is required and cannot be empty".to_string(),
        ));
    }

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
    let common_dir = git::common_dir(&repository);

    // Initialize storage
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());

    // Check if sv is initialized
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
        ));
    }

    // Load existing leases
    let existing_leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(existing_leases);

    // Expire stale leases first
    store.expire_stale();

    let mut broken = Vec::new();
    let mut not_found = Vec::new();

    for id_str in &options.ids {
        // Try to parse as UUID (full or prefix)
        let lease = find_lease_by_id(&store, id_str);

        match lease {
            Some(lease_ref) => {
                // Clone info before mutating
                let info = BrokenLeaseInfo {
                    id: lease_ref.id.to_string(),
                    pathspec: lease_ref.pathspec.clone(),
                    actor: lease_ref.actor.clone(),
                    strength: lease_ref.strength.to_string(),
                    reason: options.reason.clone(),
                };

                // Find and break the lease
                let lease_id = lease_ref.id;
                if let Some(lease_mut) = store.find_mut(&lease_id) {
                    lease_mut.break_lease(&options.reason);
                }

                broken.push(info);
            }
            None => {
                not_found.push(id_str.clone());
            }
        }
    }

    // Save updated leases - rewrite all leases since we modified in-place
    if !broken.is_empty() {
        write_leases_jsonl(&storage.leases_file(), store.all())?;

        // Record in oplog
        let oplog = OpLog::for_storage(&storage);
        let mut record = OpRecord::new("lease break", options.actor.clone());
        record.outcome = OpOutcome::success();
        record.undo_data = Some(UndoData {
            lease_changes: broken
                .iter()
                .map(|b| LeaseChange {
                    lease_id: b.id.clone(),
                    action: format!("break: {}", b.reason),
                })
                .collect(),
            ..Default::default()
        });
        oplog.append(&record)?;
    }

    // Output results
    let report = BreakReport {
        broken: broken.clone(),
        not_found: not_found.clone(),
    };

    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if !broken.is_empty() {
            println!("Broken {} lease(s):", broken.len());
            for info in &broken {
                let short_id = info.id.split('-').next().unwrap_or(&info.id);
                let actor_display = info.actor.as_deref().unwrap_or("(ownerless)");
                println!(
                    "  {} {} (was held by {})",
                    short_id, info.pathspec, actor_display
                );
            }
            println!("\nReason: {}", options.reason);
        }
        if !not_found.is_empty() {
            println!("\nNot found ({}):", not_found.len());
            for id in &not_found {
                println!("  {}", id);
            }
        }
    }

    // Return error if nothing was broken
    if broken.is_empty() && !not_found.is_empty() {
        return Err(Error::LeaseNotFound(not_found.join(", ")));
    }

    Ok(())
}

// =============================================================================
// sv lease wait
// =============================================================================

/// Options for the lease wait command
pub struct WaitOptions {
    pub targets: Vec<String>,
    pub timeout: Option<String>,
    pub poll: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(serde::Serialize)]
struct WaitReport {
    targets: Vec<String>,
    waited_ms: u64,
    timeout_ms: Option<u64>,
}

enum WaitTarget {
    Id { id: Uuid, raw: String },
    Path { path: String },
}

struct WaitTargetState {
    target: WaitTarget,
    seen: bool,
}

/// Run the lease wait command
pub fn run_wait(options: WaitOptions) -> Result<()> {
    if options.targets.is_empty() {
        return Err(Error::InvalidArgument(
            "lease wait requires at least one target".to_string(),
        ));
    }

    let poll_duration = parse_positive_duration("poll", &options.poll)?;
    let timeout_duration = match options.timeout.as_deref() {
        Some(timeout) => Some(parse_positive_duration("timeout", timeout)?),
        None => None,
    };

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

    let common_dir = resolve_common_dir(&repository)?;
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());

    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
        ));
    }

    let mut targets = parse_wait_targets(options.targets.clone())?;
    let start_time = Instant::now();

    loop {
        let existing_leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
        let mut store = LeaseStore::from_vec(existing_leases);
        store.expire_stale();

        let (active_targets, missing_ids) = wait_target_statuses(&store, &mut targets);

        if !missing_ids.is_empty() {
            return Err(Error::LeaseNotFound(missing_ids.join(", ")));
        }

        if active_targets.is_empty() {
            let waited_ms = start_time.elapsed().as_millis() as u64;
            if options.json {
                let report = WaitReport {
                    targets: options.targets.clone(),
                    waited_ms,
                    timeout_ms: timeout_duration.map(|d| d.as_millis() as u64),
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if !options.quiet {
                let waited = format_elapsed(start_time.elapsed());
                if waited_ms == 0 {
                    println!("No active leases for {}", options.targets.join(", "));
                } else {
                    println!(
                        "Leases expired for {} (waited {})",
                        options.targets.join(", "),
                        waited
                    );
                }
            }
            return Ok(());
        }

        if let Some(timeout) = timeout_duration {
            if start_time.elapsed() >= timeout {
                return Err(Error::OperationFailed(format!(
                    "timed out after {} waiting for leases: {}",
                    format_elapsed(timeout),
                    active_targets.join(", ")
                )));
            }
        }

        sleep_with_timeout(poll_duration, timeout_duration, start_time);
    }
}

fn parse_wait_targets(targets: Vec<String>) -> Result<Vec<WaitTargetState>> {
    let mut parsed = Vec::new();

    for target in targets {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument(
                "lease wait target cannot be empty".to_string(),
            ));
        }

        if let Ok(uuid) = Uuid::parse_str(trimmed) {
            parsed.push(WaitTargetState {
                target: WaitTarget::Id {
                    id: uuid,
                    raw: trimmed.to_string(),
                },
                seen: false,
            });
        } else {
            parsed.push(WaitTargetState {
                target: WaitTarget::Path {
                    path: trimmed.to_string(),
                },
                seen: true,
            });
        }
    }

    Ok(parsed)
}

fn wait_target_statuses(
    store: &LeaseStore,
    targets: &mut [WaitTargetState],
) -> (Vec<String>, Vec<String>) {
    let mut active_targets = Vec::new();
    let mut missing_ids = Vec::new();

    for target in targets {
        match &target.target {
            WaitTarget::Id { id, raw } => {
                if let Some(lease) = store.find(id) {
                    target.seen = true;
                    if lease.is_active() {
                        active_targets.push(raw.clone());
                    }
                } else if !target.seen {
                    missing_ids.push(raw.clone());
                }
            }
            WaitTarget::Path { path } => {
                if store.overlapping_path(path).next().is_some() {
                    active_targets.push(path.clone());
                }
            }
        }
    }

    (active_targets, missing_ids)
}

fn parse_positive_duration(label: &str, value: &str) -> Result<StdDuration> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidArgument(format!("{label} cannot be empty")));
    }

    let duration = parse_duration(trimmed)?;
    if duration <= chrono::Duration::zero() {
        return Err(Error::InvalidArgument(format!("{label} must be positive")));
    }

    duration
        .to_std()
        .map_err(|_| Error::InvalidArgument(format!("{label} must be positive")))
}

fn format_elapsed(duration: StdDuration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn sleep_with_timeout(poll: StdDuration, timeout: Option<StdDuration>, start_time: Instant) {
    if let Some(limit) = timeout {
        let elapsed = start_time.elapsed();
        if elapsed >= limit {
            return;
        }
        let remaining = limit - elapsed;
        let sleep_for = if poll > remaining { remaining } else { poll };
        if !sleep_for.is_zero() {
            std::thread::sleep(sleep_for);
        }
    } else {
        std::thread::sleep(poll);
    }
}

/// Find a lease by full UUID or prefix
fn find_lease_by_id<'a>(store: &'a LeaseStore, id_str: &str) -> Option<&'a Lease> {
    // Try exact UUID match first
    if let Ok(uuid) = Uuid::parse_str(id_str) {
        return store.find(&uuid);
    }

    // Try prefix match
    let normalized = id_str.to_lowercase();
    store
        .active()
        .find(|lease| lease.id.to_string().to_lowercase().starts_with(&normalized))
}

/// Write leases to file in JSONL format (atomic write via temp + rename)
fn write_leases_jsonl(path: &std::path::Path, leases: &[Lease]) -> Result<()> {
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
