//! sv switch command implementation
//!
//! Resolves a workspace by name and emits a path for quick switching.

use std::io;
use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::storage::Storage;

/// Options for the switch command
pub struct SwitchOptions {
    pub name: Option<String>,
    pub path_only: bool,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct SwitchOutput {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub base: String,
    pub last_active: String,
}

pub fn run(options: SwitchOptions) -> Result<()> {
    if options.path_only && options.json {
        return Err(Error::InvalidArgument(
            "--path cannot be combined with --json".to_string(),
        ));
    }

    let repo = git::open_repo(options.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = git::common_dir(&repo);

    let storage = Storage::new(workdir.clone(), common_dir, workdir);
    let workspace = resolve_workspace_name(&storage, options.name.as_deref(), options.json)?;
    let output = switch_workspace(&storage, &workspace)?;

    if options.path_only {
        println!("{}", output.path.display());
        return Ok(());
    }

    let mut human = HumanOutput::new(format!("sv switch: {}", output.name));
    human.push_summary("workspace", output.name.clone());
    human.push_summary("path", output.path.display().to_string());
    human.push_summary("branch", output.branch.clone());
    human.push_summary("base", output.base.clone());
    human.push_summary("last active", output.last_active.clone());
    human.push_next_step(format!("cd {}", output.path.display()));

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "switch",
        &output,
        Some(&human),
    )
}

fn resolve_workspace_name(
    storage: &Storage,
    requested: Option<&str>,
    json: bool,
) -> Result<String> {
    match requested.map(str::trim) {
        Some(name) if name.is_empty() => Err(Error::InvalidArgument(
            "workspace name cannot be empty".to_string(),
        )),
        Some(name) => Ok(name.to_string()),
        None => select_workspace_name(storage, json),
    }
}

fn select_workspace_name(storage: &Storage, json: bool) -> Result<String> {
    let mut workspaces = storage
        .list_workspaces()?
        .into_iter()
        .filter(|entry| entry.path.exists())
        .collect::<Vec<_>>();

    if workspaces.is_empty() {
        return Err(Error::InvalidArgument(
            "no active workspaces found; create one with `sv ws new <name>`".to_string(),
        ));
    }

    workspaces.sort_by(|left, right| left.name.cmp(&right.name));

    if !json {
        eprintln!("Select workspace:");
        for (index, entry) in workspaces.iter().enumerate() {
            let last_active = entry.last_active.as_deref().unwrap_or("never");
            eprintln!(
                "  {}. {} ({}) [last active: {}]",
                index + 1,
                entry.name,
                entry.path.display(),
                last_active
            );
        }
        eprint!("Enter workspace number or name: ");
    }

    let mut selection = String::new();
    let bytes_read = io::stdin().read_line(&mut selection)?;
    if bytes_read == 0 {
        return Err(Error::InvalidArgument(
            "workspace name is required".to_string(),
        ));
    }

    let selection = selection.trim();
    if selection.is_empty() {
        return Err(Error::InvalidArgument(
            "workspace selection cannot be empty".to_string(),
        ));
    }

    if let Ok(index) = selection.parse::<usize>() {
        if index == 0 || index > workspaces.len() {
            return Err(Error::InvalidArgument(format!(
                "invalid selection '{}'; choose 1-{}",
                selection,
                workspaces.len()
            )));
        }
        return Ok(workspaces[index - 1].name.clone());
    }

    if let Some(entry) = workspaces.iter().find(|entry| entry.name == selection) {
        return Ok(entry.name.clone());
    }

    Err(Error::WorkspaceNotFound(selection.to_string()))
}

fn switch_workspace(storage: &Storage, name: &str) -> Result<SwitchOutput> {
    let now = Utc::now().to_rfc3339();
    let mut output = None;

    storage.update_workspace(name, |entry| {
        entry.last_active = Some(now.clone());
        output = Some(SwitchOutput {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch: entry.branch.clone(),
            base: entry.base.clone(),
            last_active: now.clone(),
        });
        Ok(())
    })?;

    output.ok_or_else(|| Error::WorkspaceNotFound(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;
    use crate::storage::WorkspaceEntry;
    use git2::Repository;
    use tempfile::TempDir;

    #[test]
    fn switch_updates_last_active() {
        let temp = TempDir::new().expect("tempdir");
        let repo = Repository::init(temp.path()).expect("init repo");
        let repo_root = temp.path().to_path_buf();
        let common_dir = git::common_dir(&repo);
        let storage = Storage::new(repo_root.clone(), common_dir, repo_root.clone());

        let workspace_path = repo_root.join(".sv").join("worktrees").join("ws1");
        std::fs::create_dir_all(&workspace_path).expect("workspace dir");

        let entry = WorkspaceEntry::new(
            "ws1".to_string(),
            workspace_path.clone(),
            "main".to_string(),
            "main".to_string(),
            None,
            Utc::now().to_rfc3339(),
            None,
        );
        storage.add_workspace(entry).expect("add workspace");

        let output = switch_workspace(&storage, "ws1").expect("switch workspace");
        assert_eq!(output.name, "ws1");
        assert_eq!(output.path, workspace_path);
        assert!(!output.last_active.is_empty());

        let updated = storage
            .find_workspace("ws1")
            .expect("find workspace")
            .expect("workspace entry");
        assert_eq!(
            updated.last_active.as_deref(),
            Some(output.last_active.as_str())
        );
    }

    #[test]
    fn resolve_workspace_name_uses_explicit_value() {
        let temp = TempDir::new().expect("tempdir");
        let repo = Repository::init(temp.path()).expect("init repo");
        let repo_root = temp.path().to_path_buf();
        let storage = Storage::new(repo_root.clone(), git::common_dir(&repo), repo_root);

        let resolved =
            resolve_workspace_name(&storage, Some("ws-explicit"), false).expect("resolve name");
        assert_eq!(resolved, "ws-explicit");
    }
}
