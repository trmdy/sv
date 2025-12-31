//! sv onto command implementation
//!
//! Repositions the current workspace on top of another workspace's tip.

use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

use serde::Serialize;

use crate::actor;
use crate::error::{Error, Result};
use crate::git;
use crate::oplog::{OpLog, OpRecord, RefUpdate, UndoData};
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::storage::Storage;

/// Options for the onto command
pub struct OntoOptions {
    pub target_workspace: String,
    pub strategy: String,
    pub base: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum OntoStrategy {
    Rebase,
    Merge,
    CherryPick,
}

impl FromStr for OntoStrategy {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rebase" => Ok(OntoStrategy::Rebase),
            "merge" => Ok(OntoStrategy::Merge),
            "cherry-pick" | "cherrypick" => Ok(OntoStrategy::CherryPick),
            _ => Err(Error::InvalidArgument(format!(
                "invalid strategy '{}': must be rebase, merge, or cherry-pick",
                s
            ))),
        }
    }
}

#[derive(Debug, Serialize)]
struct OntoReport {
    current_workspace: String,
    target_workspace: String,
    current_branch: String,
    target_branch: String,
    base: String,
    strategy: OntoStrategy,
    head_before: Option<String>,
    head_after: Option<String>,
}

pub fn run(options: OntoOptions) -> Result<()> {
    let repo = git::open_repo(options.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = git::common_dir(&repo);
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());

    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
        ));
    }

    let registry = storage.read_workspaces()?;
    let current_entry = registry
        .workspaces
        .iter()
        .find(|entry| entry.path == workdir)
        .cloned()
        .ok_or_else(|| {
            Error::OperationFailed("workspace not registered. Run 'sv ws here'.".to_string())
        })?;

    let target_entry = registry
        .find(&options.target_workspace)
        .cloned()
        .ok_or_else(|| Error::WorkspaceNotFound(options.target_workspace.clone()))?;

    if current_entry.name == target_entry.name {
        return Err(Error::InvalidArgument(
            "target workspace must be different from current workspace".to_string(),
        ));
    }

    let base_ref = options
        .base
        .clone()
        .unwrap_or_else(|| current_entry.base.clone());
    let actor_name = actor::resolve_actor(Some(&workdir), options.actor.as_deref())?;

    let strategy = OntoStrategy::from_str(&options.strategy)?;

    let head_ref = repo.head().ok().and_then(|h| h.name().map(String::from));
    let head_before = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string());

    let mut cmd = Command::new("git");
    cmd.current_dir(&workdir);

    match strategy {
        OntoStrategy::Rebase => {
            cmd.args([
                "rebase",
                "--onto",
                &target_entry.branch,
                &base_ref,
            ]);
        }
        OntoStrategy::Merge => {
            cmd.args(["merge", &target_entry.branch]);
        }
        OntoStrategy::CherryPick => {
            return Err(Error::InvalidArgument(
                "cherry-pick strategy not yet supported; use rebase or merge".to_string(),
            ));
        }
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "git operation failed".to_string()
        } else {
            format!("git operation failed: {stderr}")
        };
        return Err(Error::OperationFailed(message));
    }

    let head_after = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string());

    let report = OntoReport {
        current_workspace: current_entry.name.clone(),
        target_workspace: target_entry.name.clone(),
        current_branch: current_entry.branch.clone(),
        target_branch: target_entry.branch.clone(),
        base: base_ref.clone(),
        strategy,
        head_before: head_before.clone(),
        head_after: head_after.clone(),
    };

    let mut human = HumanOutput::new(format!(
        "sv onto: {} -> {} ({})",
        current_entry.name, target_entry.name, options.strategy
    ));
    human.push_summary("strategy", options.strategy);
    human.push_summary("current", current_entry.name.clone());
    human.push_summary("target", target_entry.name.clone());
    human.push_summary("base", base_ref.clone());
    human.push_detail(format!(
        "branch: {} -> {}",
        current_entry.branch, target_entry.branch
    ));
    if let Some(head_before) = head_before.as_deref() {
        human.push_detail(format!("head before: {}", &head_before[..8.min(head_before.len())]));
    }
    if let Some(head_after) = head_after.as_deref() {
        human.push_detail(format!("head after: {}", &head_after[..8.min(head_after.len())]));
    }
    human.push_next_step("sv risk".to_string());

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "onto",
        &report,
        Some(&human),
    )?;

    let oplog = OpLog::for_storage(&storage);
    let mut record = OpRecord::new(
        format!("sv onto {} --strategy {}", target_entry.name, options.strategy),
        Some(actor_name),
    );
    record.affected_workspaces.push(current_entry.name.clone());
    record.affected_workspaces.push(target_entry.name.clone());
    if let Some(ref_name) = head_ref {
        record.affected_refs.push(ref_name.clone());
        record.undo_data = Some(UndoData {
            ref_updates: vec![RefUpdate {
                name: ref_name,
                old: head_before,
                new: head_after,
            }],
            ..UndoData::default()
        });
    }
    let _ = oplog.append(&record);

    Ok(())
}
