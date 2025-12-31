//! Workspace (worktree) management commands
//!
//! Implements `sv ws new`, `sv ws list`, `sv ws info`, `sv ws rm`, `sv ws here`.

use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::storage::{Storage, WorkspaceEntry};

/// Options for `sv ws new`
pub struct NewOptions {
    pub name: String,
    pub base: Option<String>,
    pub dir: Option<PathBuf>,
    pub branch: Option<String>,
    pub sparse: Vec<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Output for `sv ws new` command
#[derive(Debug, Serialize)]
pub struct NewOutput {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub base: String,
}

/// Run `sv ws new` command
///
/// Creates a new workspace (Git worktree) with:
/// 1. A new Git worktree directory
/// 2. A new branch (default: sv/ws/<name>)
/// 3. A registry entry in .git/sv/workspaces.json
pub fn run_new(opts: NewOptions) -> Result<()> {
    // Open the repository
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = repo.path().to_path_buf();

    // Load config for defaults
    let config = Config::load(&workdir)?;

    // Determine the base ref (what to branch from)
    let base_ref = opts
        .base
        .or_else(|| config.workspace.default_base.clone())
        .unwrap_or_else(|| "HEAD".to_string());

    // Determine the branch name
    let branch_name = opts
        .branch
        .unwrap_or_else(|| format!("sv/ws/{}", opts.name));

    // Determine the worktree directory path
    let worktree_path = if let Some(dir) = opts.dir {
        if dir.is_absolute() {
            dir
        } else {
            workdir.join(dir)
        }
    } else {
        // Default: <worktrees_dir>/<name>
        let worktrees_dir = config
            .workspace
            .worktrees_dir
            .clone()
            .unwrap_or_else(|| "worktrees".to_string());
        workdir.join(worktrees_dir).join(&opts.name)
    };

    // Check if workspace name already exists in registry
    let storage = Storage::new(workdir.clone(), git_dir, workdir.clone());
    if storage.find_workspace(&opts.name)?.is_some() {
        return Err(Error::InvalidArgument(format!(
            "workspace '{}' already exists in registry",
            opts.name
        )));
    }

    // Create the worktree using git module
    git::create_worktree(&repo, &opts.name, &worktree_path, &base_ref, Some(&branch_name))?;

    // Register in the workspaces registry
    let now = Utc::now().to_rfc3339();
    let entry = WorkspaceEntry::new(
        opts.name.clone(),
        worktree_path.clone(),
        branch_name.clone(),
        base_ref.clone(),
        opts.actor,
        now,
        None,
    );
    storage.add_workspace(entry)?;

    // Initialize workspace-local .sv/ directory
    let ws_storage = Storage::new(workdir.clone(), storage.shared_dir().parent().unwrap().to_path_buf(), worktree_path.clone());
    ws_storage.init_local()?;

    // Output result
    let output = NewOutput {
        name: opts.name,
        path: worktree_path,
        branch: branch_name,
        base: base_ref,
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        println!("Created workspace '{}' at {}", output.name, output.path.display());
        println!("  Branch: {}", output.branch);
        println!("  Base: {}", output.base);
    }

    Ok(())
}

/// Options for `sv ws list`
pub struct ListOptions {
    pub selector: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Output for a single workspace in list
#[derive(Debug, Serialize)]
pub struct WorkspaceListItem {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub actor: Option<String>,
    pub exists: bool,
}

/// Run `sv ws list` command
pub fn run_list(opts: ListOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = repo.path().to_path_buf();

    let storage = Storage::new(workdir.clone(), git_dir, workdir);
    let registry = storage.read_workspaces()?;

    // Convert to list items
    let items: Vec<WorkspaceListItem> = registry
        .workspaces
        .iter()
        .map(|entry| WorkspaceListItem {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch: entry.branch.clone(),
            actor: entry.actor.clone(),
            exists: entry.path.exists(),
        })
        .collect();

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if !opts.quiet {
        if items.is_empty() {
            println!("No workspaces registered");
        } else {
            for item in &items {
                let status = if item.exists { "" } else { " (missing)" };
                let actor = item
                    .actor
                    .as_ref()
                    .map(|a| format!(" [{}]", a))
                    .unwrap_or_default();
                println!("{}{}{}", item.name, actor, status);
            }
        }
    }

    Ok(())
}

/// Options for `sv ws info`
pub struct InfoOptions {
    pub name: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Detailed workspace info output
#[derive(Debug, Serialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub base: String,
    pub actor: Option<String>,
    pub created_at: String,
    pub last_active: Option<String>,
    pub exists: bool,
    pub git_status: Option<String>,
}

/// Run `sv ws info` command
pub fn run_info(opts: InfoOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = repo.path().to_path_buf();

    let storage = Storage::new(workdir.clone(), git_dir, workdir);
    let entry = storage
        .find_workspace(&opts.name)?
        .ok_or_else(|| Error::WorkspaceNotFound(opts.name.clone()))?;

    let exists = entry.path.exists();

    // Try to get git status for the workspace
    let git_status = if exists {
        match git2::Repository::open(&entry.path) {
            Ok(ws_repo) => {
                let head = ws_repo.head().ok();
                head.and_then(|h| h.shorthand().map(String::from))
            }
            Err(_) => None,
        }
    } else {
        None
    };

    let info = WorkspaceInfo {
        id: entry.id,
        name: entry.name,
        path: entry.path,
        branch: entry.branch,
        base: entry.base,
        actor: entry.actor,
        created_at: entry.created_at,
        last_active: entry.last_active,
        exists,
        git_status,
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else if !opts.quiet {
        println!("Workspace: {}", info.name);
        println!("  ID: {}", info.id);
        println!("  Path: {}", info.path.display());
        println!("  Branch: {}", info.branch);
        println!("  Base: {}", info.base);
        if let Some(actor) = &info.actor {
            println!("  Actor: {}", actor);
        }
        println!("  Created: {}", info.created_at);
        if let Some(last_active) = &info.last_active {
            println!("  Last active: {}", last_active);
        }
        println!("  Exists: {}", info.exists);
        if let Some(status) = &info.git_status {
            println!("  Git HEAD: {}", status);
        }
    }

    Ok(())
}

/// Options for `sv ws rm`
pub struct RmOptions {
    pub name: String,
    pub force: bool,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Output for `sv ws rm`
#[derive(Debug, Serialize)]
pub struct RmOutput {
    pub name: String,
    pub path: PathBuf,
    pub removed: bool,
}

/// Run `sv ws rm` command
pub fn run_rm(opts: RmOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = repo.path().to_path_buf();

    let storage = Storage::new(workdir.clone(), git_dir, workdir);

    // Find the workspace in registry
    let entry = storage
        .find_workspace(&opts.name)?
        .ok_or_else(|| Error::WorkspaceNotFound(opts.name.clone()))?;

    let path = entry.path.clone();

    // Try to remove the Git worktree
    let worktree_removed = if path.exists() {
        match git::remove_worktree(&repo, &opts.name, opts.force) {
            Ok(_) => true,
            Err(e) => {
                if opts.force {
                    // Force removal: just delete the directory
                    std::fs::remove_dir_all(&path).ok();
                    true
                } else {
                    return Err(e);
                }
            }
        }
    } else {
        // Path doesn't exist, prune worktree reference
        git::prune_worktrees(&repo).ok();
        true
    };

    // Remove from registry
    storage.remove_workspace(&opts.name)?;

    let output = RmOutput {
        name: opts.name,
        path,
        removed: worktree_removed,
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        println!("Removed workspace '{}' at {}", output.name, output.path.display());
    }

    Ok(())
}

/// Options for `sv ws here`
pub struct HereOptions {
    pub name: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Run `sv ws here` command
///
/// Registers the current directory as a workspace.
pub fn run_here(opts: HereOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = repo.path().to_path_buf();

    // Get current directory (the workspace path)
    let current_dir = std::env::current_dir()?;

    // Derive name from directory if not provided
    let name = opts.name.unwrap_or_else(|| {
        current_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string()
    });

    // Get current branch
    let head_info = git::head_info(&repo)?;
    let branch = head_info.shorthand.unwrap_or_else(|| "HEAD".to_string());

    // Get base (we'll use the branch name as base since we're registering existing)
    let base = branch.clone();

    let storage = Storage::new(workdir.clone(), git_dir, workdir);

    // Check if already registered
    if storage.find_workspace(&name)?.is_some() {
        return Err(Error::InvalidArgument(format!(
            "workspace '{}' already exists in registry",
            name
        )));
    }

    // Register
    let now = Utc::now().to_rfc3339();
    let entry = WorkspaceEntry::new(
        name.clone(),
        current_dir.clone(),
        branch.clone(),
        base.clone(),
        opts.actor,
        now,
        None,
    );
    storage.add_workspace(entry)?;

    let output = NewOutput {
        name: name.clone(),
        path: current_dir,
        branch,
        base,
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        println!("Registered current directory as workspace '{}'", name);
        println!("  Path: {}", output.path.display());
        println!("  Branch: {}", output.branch);
    }

    Ok(())
}
