//! sv switch command implementation
//!
//! Resolves a workspace by name and emits a path for quick switching.

use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::storage::Storage;

/// Options for the switch command
pub struct SwitchOptions {
    pub name: String,
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
    let output = switch_workspace(&storage, &options.name)?;

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
}
