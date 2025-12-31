//! sv status command implementation
//!
//! Provides a single-pane summary of the current workspace state.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::actor;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::lease::{Lease, LeaseStatus, LeaseStore};
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::protect::load_override;
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
}

#[derive(serde::Serialize)]
struct WorkspaceSummary {
    name: String,
    path: PathBuf,
    base: String,
}

#[derive(serde::Serialize)]
struct LeaseSummary {
    active: usize,
    expired: usize,
    conflicts: usize,
}

pub fn run(options: StatusOptions) -> Result<()> {
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    let repository = git::open_repo(Some(start.as_path()))?;
    let workdir = git::workdir(&repository)?;

    let common_dir = resolve_common_dir(&repository)?;
    let storage = Storage::new(workdir.clone(), common_dir, workdir.clone());

    let config = Config::load_from_repo(&workdir);

    let actor_name = actor::resolve_actor(Some(&workdir), options.actor.as_deref())?;

    let workspaces = if storage.is_initialized() {
        storage.list_workspaces()?
    } else {
        Vec::new()
    };

    let workspace_entry = workspaces
        .iter()
        .find(|entry| entry.path == workdir)
        .cloned();

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

    let workspace_summary = WorkspaceSummary {
        name: workspace_name.clone(),
        path: workspace_path,
        base: workspace_base.clone(),
    };

    let leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(leases);
    store.expire_stale();

    let active_leases: Vec<&Lease> = store
        .active()
        .filter(|lease| lease.actor.as_deref() == Some(actor_name.as_str()))
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

    let report = StatusReport {
        actor: actor_name.clone(),
        workspace: workspace_summary,
        leases: LeaseSummary {
            active: active_leases.len(),
            expired: expired_count,
            conflicts: conflict_ids.len(),
        },
        protect_overrides: override_count,
    };

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

    if workspace_entry.is_none() {
        warnings.push("workspace not registered".to_string());
        next_steps.push("sv ws here --name <workspace>".to_string());
    }

    if expired_count > 0 {
        warnings.push(format!("expired leases detected: {expired_count}"));
    }

    if !conflict_ids.is_empty() {
        warnings.push(format!("lease conflicts detected: {}", conflict_ids.len()));
    }

    if actor_name != "unknown" {
        next_steps.push(format!("sv lease ls --actor {actor_name}"));
    }
    next_steps.push("sv protect status".to_string());

    let header = if !storage.is_initialized() {
        "sv status: sv not initialized".to_string()
    } else if workspace_entry.is_none() {
        "sv status: workspace not registered".to_string()
    } else {
        "sv status: workspace ready".to_string()
    };

    let mut human = HumanOutput::new(header);
    human.push_summary("actor", actor_name);
    human.push_summary(
        "workspace",
        format!("{} ({})", workspace_name, workdir.display()),
    );
    human.push_summary("base", workspace_base);
    human.push_summary("repo", workdir.display().to_string());

    human.push_detail(format!("active leases: {}", active_leases.len()));
    human.push_detail(format!("protected overrides: {override_count}"));

    for warning in warnings {
        human.push_warning(warning);
    }
    for step in next_steps {
        human.push_next_step(step);
    }

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
