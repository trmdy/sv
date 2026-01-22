//! Workspace (worktree) management commands
//!
//! Implements `sv ws new`, `sv ws list`, `sv ws info`, `sv ws rm`, `sv ws clean`, `sv ws here`.

use std::path::{Path, PathBuf};

use chrono::Utc;
use git2::Repository;
use serde::Serialize;
use std::collections::HashSet;

use crate::change_id::find_change_id;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::oplog::{OpLog, OpRecord, UndoData, WorkspaceChange};
use crate::storage::{Storage, WorkspaceEntry};

/// Options for `sv ws new`
pub struct NewOptions {
    pub name: String,
    pub base: Option<String>,
    pub dir: Option<PathBuf>,
    pub branch: Option<String>,
    #[allow(dead_code)]
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
    let common_dir = resolve_common_dir(&repo)?;

    // Load config for defaults
    let config = Config::load_from_repo(&workdir);

    // Determine the base ref (what to branch from)
    let base_ref = opts.base.unwrap_or_else(|| config.base.clone());

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
        // Default: .sv/worktrees/<name>
        workdir.join(".sv").join("worktrees").join(&opts.name)
    };

    // Check if workspace name already exists in registry
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());
    if storage.find_workspace(&opts.name)?.is_some() {
        return Err(Error::InvalidArgument(format!(
            "workspace '{}' already exists in registry",
            opts.name
        )));
    }

    // Create the worktree using git module
    git::create_worktree(
        &repo,
        &opts.name,
        &worktree_path,
        &base_ref,
        Some(&branch_name),
    )?;

    // Register in the workspaces registry
    let now = Utc::now().to_rfc3339();
    let actor = opts.actor.clone();
    let entry = WorkspaceEntry::new(
        opts.name.clone(),
        worktree_path.clone(),
        branch_name.clone(),
        base_ref.clone(),
        actor.clone(),
        now,
        None,
    );
    storage.add_workspace(entry)?;

    // Initialize workspace-local .sv/ directory
    let ws_storage = Storage::new(workdir.clone(), common_dir, worktree_path.clone());
    ws_storage.init_local()?;

    // Record operation in oplog
    let oplog = OpLog::for_storage(&storage);
    let mut record = OpRecord::new(format!("sv ws new {}", opts.name), actor.clone());
    record.affected_workspaces.push(opts.name.clone());
    record.affected_refs.push(branch_name.clone());
    record.undo_data = Some(UndoData {
        workspace_changes: vec![WorkspaceChange {
            name: opts.name.clone(),
            action: "create".to_string(),
            path: Some(worktree_path.display().to_string()),
            branch: Some(branch_name.clone()),
            base: Some(base_ref.clone()),
        }],
        created_paths: vec![worktree_path.display().to_string()],
        ..Default::default()
    });
    // Best-effort oplog write - don't fail the command if oplog fails
    let _ = oplog.append(&record);

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
        println!(
            "Created workspace '{}' at {}",
            output.name,
            output.path.display()
        );
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
    pub base: String,
    pub actor: Option<String>,
    pub last_active: Option<String>,
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead_behind: Option<AheadBehind>,
}

/// Ahead/behind status for a workspace branch against its base.
#[derive(Debug, Serialize)]
pub struct AheadBehind {
    pub base: String,
    pub ahead: usize,
    pub behind: usize,
}

/// Run `sv ws list` command
pub fn run_list(opts: ListOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = resolve_common_dir(&repo)?;

    let storage = Storage::new(workdir.clone(), common_dir, workdir);
    let registry = storage.read_workspaces()?;
    let entries = match opts.selector.as_deref() {
        Some(selector) => super::resolve_hoist_workspaces(&repo, &registry, selector)?,
        None => registry.workspaces.clone(),
    };

    // Convert to list items
    let items: Vec<WorkspaceListItem> = entries
        .iter()
        .map(|entry| {
            let ahead_behind = compute_ahead_behind(&repo, &entry.branch, &entry.base);
            WorkspaceListItem {
                name: entry.name.clone(),
                path: entry.path.clone(),
                branch: entry.branch.clone(),
                base: entry.base.clone(),
                actor: entry.actor.clone(),
                last_active: entry.last_active.clone(),
                exists: entry.path.exists(),
                ahead_behind,
            }
        })
        .collect();

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if !opts.quiet {
        if items.is_empty() {
            println!("No workspaces registered");
        } else {
            for item in &items {
                let missing = if item.exists { "" } else { " (missing)" };
                println!("{}{}", item.name, missing);
                println!("  path: {}", item.path.display());
                println!("  branch: {}", item.branch);
                println!("  base: {}", item.base);
                if let Some(actor) = &item.actor {
                    println!("  actor: {}", actor);
                }
                if let Some(last_active) = &item.last_active {
                    println!("  last active: {}", last_active);
                }
                if let Some(status) = &item.ahead_behind {
                    println!(
                        "  status: {} ahead / {} behind vs {}",
                        status.ahead, status.behind, status.base
                    );
                }
            }
        }
    }

    Ok(())
}

fn compute_ahead_behind(repo: &Repository, branch: &str, base: &str) -> Option<AheadBehind> {
    let branch_oid = resolve_oid(repo, branch)?;
    let base_oid = resolve_oid(repo, base)?;
    let (ahead, behind) = repo.graph_ahead_behind(branch_oid, base_oid).ok()?;
    Some(AheadBehind {
        base: base.to_string(),
        ahead: ahead as usize,
        behind: behind as usize,
    })
}

fn resolve_oid(repo: &Repository, spec: &str) -> Option<git2::Oid> {
    let obj = repo.revparse_single(spec).ok()?;
    let commit = obj.peel_to_commit().ok()?;
    Some(commit.id())
}

fn collect_change_ids(repo: &Repository, branch: &str, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    let head = match repo
        .revparse_single(branch)
        .and_then(|obj| obj.peel_to_commit())
    {
        Ok(commit) => commit,
        Err(_) => return results,
    };

    let mut revwalk = match repo.revwalk() {
        Ok(walk) => walk,
        Err(_) => return results,
    };
    if revwalk.push(head.id()).is_err() {
        return results;
    }

    for oid in revwalk.take(limit) {
        let oid = match oid {
            Ok(oid) => oid,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(oid) {
            Ok(commit) => commit,
            Err(_) => continue,
        };
        let message = commit.message().unwrap_or_default();
        if let Some(change_id) = find_change_id(message) {
            if seen.insert(change_id.clone()) {
                results.push(change_id);
            }
        }
    }

    results
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
    /// Current git HEAD (shorthand branch name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_head: Option<String>,
    /// Files touched (changed vs base)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub touched_paths: Vec<String>,
    /// Leases affecting this workspace
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub leases: Vec<WorkspaceLease>,
    /// Ahead/behind count vs base ref
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead_behind_base: Option<AheadBehind>,
    /// Ahead/behind count vs main/master
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead_behind_main: Option<AheadBehind>,
    /// Recent Change-Ids from commits
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub change_ids: Vec<String>,
}

/// Lease info for workspace display
#[derive(Debug, Serialize)]
pub struct WorkspaceLease {
    pub id: String,
    pub pathspec: String,
    pub strength: String,
    pub actor: Option<String>,
    pub expires_at: String,
}

/// Run `sv ws info` command
pub fn run_info(opts: InfoOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = resolve_common_dir(&repo)?;

    let storage = Storage::new(workdir.clone(), common_dir, workdir.clone());
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

    let touched_paths = match git::diff_files(&repo, &entry.base, Some(&entry.branch)) {
        Ok(changes) => {
            let mut paths: Vec<String> = changes
                .into_iter()
                .map(|change| change.path.to_string_lossy().to_string())
                .collect();
            paths.sort();
            paths
        }
        Err(_) => Vec::new(),
    };

    let leases = if touched_paths.is_empty() {
        Vec::new()
    } else {
        match storage.load_leases() {
            Ok(store) => store
                .active()
                .filter(|lease| touched_paths.iter().any(|path| lease.matches_path(path)))
                .map(|lease| WorkspaceLease {
                    id: lease.id.to_string(),
                    pathspec: lease.pathspec.clone(),
                    strength: lease.strength.to_string(),
                    actor: lease.actor.clone(),
                    expires_at: lease.expires_at.to_rfc3339(),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    };

    let ahead_behind_base = compute_ahead_behind(&repo, &entry.branch, &entry.base);
    let main_ref = Config::load_from_repo(&workdir).base;
    let ahead_behind_main = if main_ref != entry.base {
        compute_ahead_behind(&repo, &entry.branch, &main_ref)
    } else {
        None
    };

    let change_ids = collect_change_ids(&repo, &entry.branch, 10);

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
        git_head: git_status,
        touched_paths,
        leases,
        ahead_behind_base,
        ahead_behind_main,
        change_ids,
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
        if let Some(status) = &info.git_head {
            println!("  Git HEAD: {}", status);
        }
        if let Some(status) = &info.ahead_behind_base {
            println!(
                "  Ahead/behind vs {}: {} ahead / {} behind",
                status.base, status.ahead, status.behind
            );
        }
        if let Some(status) = &info.ahead_behind_main {
            println!(
                "  Ahead/behind vs {}: {} ahead / {} behind",
                status.base, status.ahead, status.behind
            );
        }
        if !info.touched_paths.is_empty() {
            println!("  Touched paths:");
            for path in &info.touched_paths {
                println!("    - {}", path);
            }
        }
        if !info.leases.is_empty() {
            println!("  Leases affecting workspace:");
            for lease in &info.leases {
                let actor = lease.actor.as_deref().unwrap_or("-");
                println!(
                    "    - {} {} [{}] actor={} expires={}",
                    lease.id, lease.pathspec, lease.strength, actor, lease.expires_at
                );
            }
        }
        if !info.change_ids.is_empty() {
            println!("  Recent Change-Ids: {}", info.change_ids.join(", "));
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

/// Workspace cleanup report (used by ws clean and hoist --rm)
#[derive(Debug, Serialize, Clone)]
pub struct WorkspaceCleanupReport {
    #[serde(skip_serializing_if = "is_false")]
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<WorkspaceCleanupFailure>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<WorkspaceCleanupSkip>,
}

#[derive(Debug, Serialize, Clone)]
pub struct WorkspaceCleanupFailure {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct WorkspaceCleanupSkip {
    pub name: String,
    pub reason: String,
}

/// Run `sv ws rm` command
pub fn run_rm(opts: RmOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = resolve_common_dir(&repo)?;

    let storage = Storage::new(workdir.clone(), common_dir, workdir);

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

    // Record operation in oplog
    let oplog = OpLog::for_storage(&storage);
    let mut record = OpRecord::new(
        format!("sv ws rm {}", opts.name),
        None, // No actor context in RmOptions currently
    );
    record.affected_workspaces.push(opts.name.clone());
    record.affected_refs.push(entry.branch.clone());
    record.undo_data = Some(UndoData {
        workspace_changes: vec![WorkspaceChange {
            name: opts.name.clone(),
            action: "remove".to_string(),
            path: Some(path.display().to_string()),
            branch: Some(entry.branch.clone()),
            base: Some(entry.base.clone()),
        }],
        deleted_paths: vec![path.display().to_string()],
        ..Default::default()
    });
    // Best-effort oplog write
    let _ = oplog.append(&record);

    let output = RmOutput {
        name: opts.name,
        path,
        removed: worktree_removed,
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        println!(
            "Removed workspace '{}' at {}",
            output.name,
            output.path.display()
        );
    }

    Ok(())
}

/// Options for `sv ws clean`
pub struct CleanOptions {
    pub selector: Option<String>,
    pub dest: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Output for `sv ws clean`
#[derive(Debug, Serialize)]
pub struct CleanOutput {
    pub selector: String,
    pub dest: Option<String>,
    pub matched: usize,
    pub merged: usize,
    pub cleanup: WorkspaceCleanupReport,
}

impl WorkspaceCleanupReport {
    pub fn new(dry_run: bool) -> Self {
        Self {
            dry_run,
            removed: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Remove workspaces by name, recording success/failure.
pub fn remove_workspaces(
    repo_root: &Path,
    workspaces: &[WorkspaceEntry],
    force: bool,
    dry_run: bool,
    current_path: &Path,
) -> WorkspaceCleanupReport {
    let mut report = WorkspaceCleanupReport::new(dry_run);

    for entry in workspaces {
        if entry.path == current_path {
            report.skipped.push(WorkspaceCleanupSkip {
                name: entry.name.clone(),
                reason: "current workspace".to_string(),
            });
            continue;
        }

        if dry_run {
            report.removed.push(entry.name.clone());
            continue;
        }

        match run_rm(RmOptions {
            name: entry.name.clone(),
            force,
            repo: Some(repo_root.to_path_buf()),
            json: false,
            quiet: true,
        }) {
            Ok(()) => report.removed.push(entry.name.clone()),
            Err(err) => report.failed.push(WorkspaceCleanupFailure {
                name: entry.name.clone(),
                error: err.to_string(),
            }),
        }
    }

    report
}

/// Run `sv ws clean` command
pub fn run_clean(opts: CleanOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = resolve_common_dir(&repo)?;
    let storage = Storage::new(workdir.clone(), common_dir, workdir.clone());
    let registry = storage.read_workspaces()?;

    let selector = opts
        .selector
        .clone()
        .unwrap_or_else(|| "ws(active)".to_string());
    let matching = super::resolve_hoist_workspaces(&repo, &registry, &selector)?;
    let matched = matching.len();

    let mut candidates = Vec::new();
    let mut skipped = Vec::new();

    for entry in matching {
        if entry.path == workdir {
            skipped.push(WorkspaceCleanupSkip {
                name: entry.name.clone(),
                reason: "current workspace".to_string(),
            });
            continue;
        }
        let dest_ref = opts.dest.as_deref().unwrap_or(&entry.base);
        match git::is_ancestor(&repo, &entry.branch, dest_ref) {
            Ok(true) => candidates.push(entry),
            Ok(false) => skipped.push(WorkspaceCleanupSkip {
                name: entry.name.clone(),
                reason: format!("not merged into {}", dest_ref),
            }),
            Err(err) => skipped.push(WorkspaceCleanupSkip {
                name: entry.name.clone(),
                reason: format!("merge check failed: {}", err),
            }),
        }
    }

    let mut cleanup = remove_workspaces(
        &workdir,
        &candidates,
        opts.force,
        opts.dry_run,
        &workdir,
    );
    cleanup.skipped.extend(skipped);

    let output = CleanOutput {
        selector,
        dest: opts.dest,
        matched,
        merged: candidates.len(),
        cleanup,
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !opts.quiet {
        let header = if opts.dry_run {
            "Workspace cleanup (dry run)"
        } else {
            "Workspace cleanup"
        };
        println!("{header}");
        println!("  Selector: {}", output.selector);
        if let Some(dest) = &output.dest {
            println!("  Dest: {}", dest);
        } else {
            println!("  Dest: workspace base");
        }
        println!("  Matched: {}", output.matched);
        println!("  Merged: {}", output.merged);
        println!("  Removed: {}", output.cleanup.removed.len());

        if !output.cleanup.removed.is_empty() {
            let label = if opts.dry_run { "Would remove" } else { "Removed" };
            println!("{label}:");
            for name in &output.cleanup.removed {
                println!("  - {}", name);
            }
        }
        if !output.cleanup.skipped.is_empty() {
            println!("Skipped:");
            for skip in &output.cleanup.skipped {
                println!("  - {} ({})", skip.name, skip.reason);
            }
        }
        if !output.cleanup.failed.is_empty() {
            println!("Failed:");
            for failure in &output.cleanup.failed {
                println!("  - {} ({})", failure.name, failure.error);
            }
        }
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

/// Ensure the current workspace is registered, auto-registering if needed.
///
/// This is the preferred way for commands to get the current workspace context.
/// If the current directory is not registered as a workspace, it will be
/// automatically registered with a name derived from the directory name.
///
/// Returns the workspace entry for the current directory.
pub fn ensure_current_workspace(
    storage: &Storage,
    repo: &git2::Repository,
    workdir: &std::path::Path,
    actor: Option<&str>,
) -> Result<WorkspaceEntry> {
    let registry = storage.read_workspaces()?;

    // Check if already registered
    if let Some(entry) = registry.workspaces.iter().find(|e| e.path == workdir) {
        return Ok(entry.clone());
    }

    // Auto-register the workspace
    // Handle unborn branches (repos with no commits yet) gracefully
    let branch = git::head_info(repo)
        .ok()
        .and_then(|info| info.shorthand)
        .unwrap_or_else(|| "HEAD".to_string());

    let name = workdir
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .unwrap_or_else(|| branch.clone());

    // Ensure the name is unique
    let final_name = if registry.find(&name).is_some() {
        // Name collision - append a suffix
        let mut candidate = name.clone();
        let mut counter = 1;
        while registry.find(&candidate).is_some() {
            candidate = format!("{}-{}", name, counter);
            counter += 1;
        }
        candidate
    } else {
        name
    };

    let base = branch.clone();

    // Ensure workspace-local state directory exists
    storage.init_local()?;

    // Register
    let now = Utc::now().to_rfc3339();
    let entry = WorkspaceEntry::new(
        final_name.clone(),
        workdir.to_path_buf(),
        branch.clone(),
        base.clone(),
        actor.map(|s| s.to_string()),
        now,
        None,
    );
    storage.add_workspace(entry.clone())?;

    // Record operation in oplog (best-effort)
    let oplog = OpLog::for_storage(storage);
    let mut record = OpRecord::new(
        format!("auto-register workspace {}", final_name),
        actor.map(|s| s.to_string()),
    );
    record.affected_workspaces.push(final_name.clone());
    record.affected_refs.push(branch.clone());
    record.undo_data = Some(UndoData {
        workspace_changes: vec![WorkspaceChange {
            name: final_name,
            action: "register".to_string(),
            path: Some(workdir.display().to_string()),
            branch: Some(branch),
            base: Some(base),
        }],
        ..Default::default()
    });
    let _ = oplog.append(&record);

    Ok(entry)
}

/// Run `sv ws here` command
///
/// Registers the current directory as a workspace.
pub fn run_here(opts: HereOptions) -> Result<()> {
    let repo = git::open_repo(opts.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = resolve_common_dir(&repo)?;

    // Derive name from directory if not provided
    // Get current branch
    let head_info = git::head_info(&repo)?;
    let branch = head_info.shorthand.unwrap_or_else(|| "HEAD".to_string());

    let derived_name = workdir
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .or_else(|| Some(branch.clone()))
        .unwrap_or_else(|| "workspace".to_string());

    let name = opts.name.unwrap_or(derived_name);

    // Get base (we'll use the branch name as base since we're registering existing)
    let base = branch.clone();

    let storage = Storage::new(workdir.clone(), common_dir, workdir.clone());

    // Ensure workspace-local state directory exists
    storage.init_local()?;

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
        workdir.clone(),
        branch.clone(),
        base.clone(),
        opts.actor.clone(),
        now,
        None,
    );
    storage.add_workspace(entry)?;

    // Record operation in oplog
    let oplog = OpLog::for_storage(&storage);
    let mut record = OpRecord::new(format!("sv ws here {}", name), opts.actor);
    record.affected_workspaces.push(name.clone());
    record.affected_refs.push(branch.clone());
    record.undo_data = Some(UndoData {
        workspace_changes: vec![WorkspaceChange {
            name: name.clone(),
            action: "register".to_string(),
            path: Some(workdir.display().to_string()),
            branch: Some(branch.clone()),
            base: Some(base.clone()),
        }],
        ..Default::default()
    });
    // Best-effort oplog write
    let _ = oplog.append(&record);

    let output = NewOutput {
        name: name.clone(),
        path: workdir,
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

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Repository, Signature};
    use tempfile::TempDir;

    fn commit_all(repo: &Repository, message: &str) {
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .expect("add");
        index.write().expect("write index");

        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("tree");
        let sig = Signature::now("sv-test", "sv-test@example.com").expect("sig");

        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        match parent {
            Some(parent) => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                .expect("commit"),
            None => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .expect("commit"),
        };
    }

    #[test]
    fn run_here_registers_repo_root_and_creates_local_state() -> Result<()> {
        let temp = TempDir::new().expect("temp dir");
        let repo = Repository::init(temp.path()).expect("init repo");

        std::fs::write(temp.path().join("README.md"), "base").expect("write readme");
        commit_all(&repo, "initial commit");

        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp.path())?;

        let result = run_here(HereOptions {
            name: None,
            actor: Some("agent1".to_string()),
            repo: None,
            json: false,
            quiet: true,
        });

        std::env::set_current_dir(original_dir)?;
        result?;

        let storage = Storage::for_repo(temp.path().to_path_buf());
        let registry = storage.read_workspaces()?;
        assert_eq!(registry.workspaces.len(), 1);

        let entry = &registry.workspaces[0];
        let expected_name = temp
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("workspace");

        assert_eq!(entry.name, expected_name);
        let expected_path = std::fs::canonicalize(temp.path())?;
        let actual_path = std::fs::canonicalize(&entry.path)?;
        assert_eq!(actual_path, expected_path);
        assert_eq!(entry.actor.as_deref(), Some("agent1"));
        assert!(storage.local_dir().exists());

        Ok(())
    }
}
