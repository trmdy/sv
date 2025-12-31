//! Worktree operations for managing workspaces.

use std::path::{Path, PathBuf};

use git2::{Repository, Worktree, WorktreeAddOptions, WorktreeLockStatus, WorktreePruneOptions};

use crate::error::{Error, Result};

/// Summary of a worktree from the current repository.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub locked: bool,
    pub lock_reason: Option<String>,
}

/// Options for creating a new worktree.
#[derive(Debug, Clone)]
pub struct WorktreeCreateOptions {
    pub reference: Option<String>,
    pub lock: bool,
    pub checkout_existing: bool,
}

impl Default for WorktreeCreateOptions {
    fn default() -> Self {
        Self {
            reference: None,
            lock: false,
            checkout_existing: false,
        }
    }
}

/// Resolve a worktree path relative to the repository workdir.
pub fn resolve_worktree_path(repo: &Repository, path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let workdir = repo.workdir().ok_or_else(|| {
        Error::OperationFailed("repository has no working directory".to_string())
    })?;
    Ok(workdir.join(path))
}

/// Enumerate existing worktrees.
pub fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>> {
    let names = repo.worktrees()?;
    let mut result = Vec::new();

    for name_opt in names.iter() {
        let name = name_opt.ok_or_else(|| {
            Error::OperationFailed("worktree name is not valid utf-8".to_string())
        })?;
        let worktree = repo.find_worktree(name)?;
        result.push(worktree_info(name, &worktree)?);
    }

    Ok(result)
}

/// Create a new worktree at the provided path.
pub fn add_worktree(
    repo: &Repository,
    name: &str,
    path: &Path,
    options: &WorktreeCreateOptions,
) -> Result<WorktreeInfo> {
    let resolved = resolve_worktree_path(repo, path)?;

    let mut add_opts = WorktreeAddOptions::new();
    add_opts.lock(options.lock);
    add_opts.checkout_existing(options.checkout_existing);

    let reference = if let Some(reference) = options.reference.as_deref() {
        Some(repo.find_reference(reference)?)
    } else {
        None
    };
    add_opts.reference(reference.as_ref());

    let worktree = repo.worktree(name, &resolved, Some(&add_opts))?;
    worktree_info(name, &worktree)
}

/// Remove a worktree by name.
pub fn remove_worktree(
    repo: &Repository,
    name: &str,
    remove_working_tree: bool,
    force: bool,
) -> Result<()> {
    let worktree = repo.find_worktree(name)?;
    let mut prune_opts = WorktreePruneOptions::new();
    prune_opts.valid(true).working_tree(remove_working_tree);
    if force {
        prune_opts.locked(true);
    }

    let prunable = worktree.is_prunable(Some(&mut prune_opts))?;
    if !prunable {
        return Err(Error::OperationFailed(format!(
            "worktree '{name}' is not prunable"
        )));
    }

    worktree.prune(Some(&mut prune_opts))?;
    Ok(())
}

fn worktree_info(name: &str, worktree: &Worktree) -> Result<WorktreeInfo> {
    let path = worktree.path().to_path_buf();
    let (locked, lock_reason) = match worktree.is_locked()? {
        WorktreeLockStatus::Unlocked => (false, None),
        WorktreeLockStatus::Locked(reason) => (true, reason),
    };

    Ok(WorktreeInfo {
        name: name.to_string(),
        path,
        locked,
        lock_reason,
    })
}
