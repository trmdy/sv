//! sv status command implementation
//!
//! Provides a single-pane summary of the current workspace state.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::actor;
use crate::cli::ws;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::lease::{Lease, LeaseStatus, LeaseStore};
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::protect::{compute_status, load_override};
use crate::storage::Storage;

/// Options for the status command
pub struct StatusOptions {
    pub repo: Option<PathBuf>,
    pub actor: Option<String>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(serde::Serialize)]
struct StatusReport {
    actor: String,
    workspace: WorkspaceSummary,
    leases: LeaseSummary,
    protect_overrides: usize,
    protected_blocking: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    protected_files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unresolved_conflicts: Vec<ConflictInfo>,
}

#[derive(serde::Serialize)]
struct ConflictInfo {
    commit_id: String,
    files: Vec<String>,
    detected_at: String,
}

#[derive(serde::Serialize)]
struct WorkspaceSummary {
    name: String,
    path: PathBuf,
    base: String,
    branch: String,
    repo_root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    ahead_behind: Option<AheadBehind>,
}

#[derive(serde::Serialize)]
struct LeaseSummary {
    active: usize,
    expired: usize,
    conflicts: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    owned: Vec<LeaseInfo>,
}

#[derive(serde::Serialize)]
struct LeaseInfo {
    id: String,
    pathspec: String,
    strength: String,
    expires_at: String,
}

#[derive(serde::Serialize, Clone)]
struct AheadBehind {
    base: String,
    ahead: usize,
    behind: usize,
}

pub fn run(options: StatusOptions) -> Result<()> {
    let start = options
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let repository = git::open_repo(Some(start.as_path()))?;
    let workdir = git::workdir(&repository)?;

    let common_dir = resolve_common_dir(&repository)?;
    let repo_root = common_dir.parent().unwrap_or(&workdir).to_path_buf();
    let storage = Storage::new(workdir.clone(), common_dir, workdir.clone());

    let config = Config::load_from_repo(&workdir);

    let actor_name = actor::resolve_actor(Some(&workdir), options.actor.as_deref())?;

    // Ensure current workspace is registered (auto-registers if needed when sv is initialized)
    let workspace_entry = if storage.is_initialized() {
        Some(ws::ensure_current_workspace(
            &storage,
            &repository,
            &workdir,
            options.actor.as_deref(),
        )?)
    } else {
        None
    };

    let workspace_name = workspace_entry
        .as_ref()
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let workspace_base = workspace_entry
        .as_ref()
        .map(|entry| entry.base.clone())
        .unwrap_or_else(|| config.base.clone());

    let workspace_path = workspace_entry
        .as_ref()
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| workdir.clone());

    let head_info = git::head_info(&repository).ok();
    let workspace_branch = workspace_entry
        .as_ref()
        .map(|entry| entry.branch.clone())
        .or_else(|| head_info.as_ref().and_then(|info| info.shorthand.clone()))
        .unwrap_or_else(|| "HEAD".to_string());

    let ahead_behind = compute_ahead_behind(&repository, &workspace_branch, &workspace_base);

    let workspace_summary = WorkspaceSummary {
        name: workspace_name.clone(),
        path: workspace_path,
        base: workspace_base.clone(),
        branch: workspace_branch.clone(),
        repo_root: repo_root.clone(),
        ahead_behind: ahead_behind.clone(),
    };

    let leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(leases);
    store.expire_stale();

    let active_leases: Vec<&Lease> = store
        .active()
        .filter(|lease| lease.actor.as_deref() == Some(actor_name.as_str()))
        .collect();

    let owned_leases: Vec<LeaseInfo> = active_leases
        .iter()
        .map(|lease| LeaseInfo {
            id: lease.id.to_string(),
            pathspec: lease.pathspec.clone(),
            strength: lease.strength.to_string(),
            expires_at: lease.expires_at.to_rfc3339(),
        })
        .collect();

    let mut conflict_ids = HashSet::new();
    for lease in &active_leases {
        let conflicts = store.check_conflicts(
            &lease.pathspec,
            lease.strength,
            Some(actor_name.as_str()),
            false,
        );
        for conflict in conflicts {
            conflict_ids.insert(conflict.id);
        }
    }

    let expired_count = store
        .all()
        .iter()
        .filter(|lease| lease.status == LeaseStatus::Expired)
        .count();

    let override_data = load_override(&storage).ok();
    let override_count = override_data
        .as_ref()
        .map(|data| data.disabled_patterns.len())
        .unwrap_or(0);

    let staged_paths = git::staged_paths(&repository).unwrap_or_default();
    let protect_status = compute_status(&config, override_data.as_ref(), &staged_paths)?;
    let mut blocked_files = HashSet::new();
    for rule_status in protect_status.rules {
        if rule_status.disabled {
            continue;
        }
        let mode = rule_status.rule.mode.as_str();
        let is_blocking = mode == "guard" || mode == "readonly";
        if !is_blocking {
            continue;
        }
        for matched_file in rule_status.matched_files {
            blocked_files.insert(matched_file.to_string_lossy().to_string());
        }
    }
    let mut protected_files: Vec<String> = blocked_files.into_iter().collect();
    protected_files.sort();

    let mut warnings = Vec::new();
    let mut next_steps = Vec::new();

    if !storage.is_initialized() {
        warnings.push("sv not initialized".to_string());
        next_steps.push("sv init".to_string());
    }

    let config_path = workdir.join(".sv.toml");
    if !config_path.exists() {
        warnings.push("missing .sv.toml; using defaults".to_string());
    }

    if actor_name == "unknown" {
        warnings.push("actor not set; using default".to_string());
        next_steps.push("sv actor set <name>".to_string());
    }

    if expired_count > 0 {
        warnings.push(format!("expired leases detected: {expired_count}"));
    }

    if !conflict_ids.is_empty() {
        warnings.push(format!("lease conflicts detected: {}", conflict_ids.len()));
    }

    if !protected_files.is_empty() {
        warnings.push(format!(
            "protected paths staged (guard): {}",
            protected_files.len()
        ));
    }

    // Load unresolved conflicts
    let unresolved_conflicts: Vec<ConflictInfo> = storage
        .unresolved_conflicts()
        .unwrap_or_default()
        .into_iter()
        .map(|c| ConflictInfo {
            commit_id: c.commit_id,
            files: c.files,
            detected_at: c.detected_at.to_rfc3339(),
        })
        .collect();

    if !unresolved_conflicts.is_empty() {
        warnings.push(format!(
            "unresolved conflicts: {} commit(s)",
            unresolved_conflicts.len()
        ));
    }

    if actor_name != "unknown" {
        next_steps.push(format!("sv lease ls --actor {actor_name}"));
    }
    next_steps.push("sv protect status".to_string());

    let header = if !storage.is_initialized() {
        "sv status: sv not initialized".to_string()
    } else {
        "sv status: workspace ready".to_string()
    };

    let mut human = HumanOutput::new(header);
    human.push_summary("actor", actor_name.clone());
    human.push_summary(
        "workspace",
        format!("{} ({})", workspace_name, workdir.display()),
    );
    human.push_summary("branch", workspace_branch.clone());
    human.push_summary("base", workspace_base);
    human.push_summary("repo", repo_root.display().to_string());

    if let Some(status) = &ahead_behind {
        human.push_detail(format!(
            "ahead/behind vs {}: {} ahead / {} behind",
            status.base, status.ahead, status.behind
        ));
    }
    human.push_detail(format!("active leases: {}", active_leases.len()));
    for lease in &owned_leases {
        human.push_detail(format!(
            "lease {} {} [{}] expires {}",
            lease.id, lease.pathspec, lease.strength, lease.expires_at
        ));
    }
    human.push_detail(format!("protected overrides: {override_count}"));
    if !protected_files.is_empty() {
        human.push_detail(format!(
            "protected paths staged (guard): {}",
            protected_files.len()
        ));
    }
    if !unresolved_conflicts.is_empty() {
        human.push_detail(format!(
            "unresolved conflicts: {} commit(s)",
            unresolved_conflicts.len()
        ));
        for conflict in &unresolved_conflicts {
            human.push_detail(format!(
                "  conflict {} - files: {}",
                &conflict.commit_id[..8.min(conflict.commit_id.len())],
                conflict.files.join(", ")
            ));
        }
    }

    for warning in warnings {
        human.push_warning(warning);
    }
    for step in next_steps {
        human.push_next_step(step);
    }

    let report = StatusReport {
        actor: actor_name.clone(),
        workspace: workspace_summary,
        leases: LeaseSummary {
            active: active_leases.len(),
            expired: expired_count,
            conflicts: conflict_ids.len(),
            owned: owned_leases,
        },
        protect_overrides: override_count,
        protected_blocking: protected_files.len(),
        protected_files,
        unresolved_conflicts,
    };

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "status",
        &report,
        Some(&human),
    )?;

    Ok(())
}

fn compute_ahead_behind(repo: &git2::Repository, branch: &str, base: &str) -> Option<AheadBehind> {
    let branch_oid = resolve_oid(repo, branch)?;
    let base_oid = resolve_oid(repo, base)?;
    let (ahead, behind) = repo.graph_ahead_behind(branch_oid, base_oid).ok()?;
    Some(AheadBehind {
        base: base.to_string(),
        ahead: ahead as usize,
        behind: behind as usize,
    })
}

fn resolve_oid(repo: &git2::Repository, spec: &str) -> Option<git2::Oid> {
    let obj = repo.revparse_single(spec).ok()?;
    let commit = obj.peel_to_commit().ok()?;
    Some(commit.id())
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
