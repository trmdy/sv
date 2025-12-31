//! sv commit command implementation
//!
//! Wraps git commit with sv-specific checks:
//! - Protected path enforcement
//! - Lease conflict checking
//! - Change-Id injection (future)
//!
//! This is the basic wrapper that passes through to git commit.

use std::path::PathBuf;
use std::process::Command;

use crate::actor;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::lease::{Lease, LeaseScope, LeaseStrength, LeaseStore};
use crate::protect;
use crate::storage::Storage;

/// Options for the commit command
pub struct CommitOptions {
    pub message: Option<String>,
    pub file: Option<PathBuf>,
    pub amend: bool,
    pub all: bool,
    pub no_edit: bool,
    pub allow_protected: bool,
    pub force_lease: bool,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of commit operation
#[derive(serde::Serialize)]
struct CommitResult {
    success: bool,
    commit_hash: Option<String>,
    message: Option<String>,
    files_committed: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    protected_files: Vec<ProtectedFileInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lease_conflicts: Vec<LeaseConflictInfo>,
}

/// Information about a protected file violation
#[derive(Clone, serde::Serialize)]
struct ProtectedFileInfo {
    file: String,
    pattern: String,
    mode: String,
}

/// Information about a lease conflict
#[derive(Clone, serde::Serialize)]
struct LeaseConflictInfo {
    file: String,
    lease_id: String,
    holder: String,
    strength: String,
}

/// Run the commit command
pub fn run(options: CommitOptions) -> Result<()> {
    // Discover repository
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    
    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;
    
    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?;

    // If -a flag, stage all modified files
    if options.all {
        stage_all_modified(&repository)?;
    }

    // Get list of files to be committed
    let staged_files = get_staged_files(&repository)?;
    
    if staged_files.is_empty() && !options.amend {
        if options.json {
            let result = CommitResult {
                success: false,
                commit_hash: None,
                message: Some("Nothing to commit".to_string()),
                files_committed: vec![],
                protected_files: vec![],
                lease_conflicts: vec![],
            };
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else if !options.quiet {
            println!("Nothing to commit (no staged files)");
        }
        return Ok(());
    }

    // Check protected paths (sv-8jf.4.5)
    let (protected_guard, protected_warn) = check_protected_paths(&repository, &staged_files)?;
    
    // Warn about warn-mode protected files
    if !protected_warn.is_empty() && !options.quiet {
        eprintln!("Warning: Committing protected files (warn mode):");
        for pf in &protected_warn {
            eprintln!("  {} (pattern: {}, mode: {})", pf.file, pf.pattern, pf.mode);
        }
    }
    
    // Block on guard-mode protected files unless --allow-protected
    if !protected_guard.is_empty() && !options.allow_protected {
        // Print human-readable error (JSON errors are handled by main)
        if !options.json && !options.quiet {
            eprintln!("Error: Commit blocked by protected paths:");
            for pf in &protected_guard {
                eprintln!("  {} (pattern: {}, mode: {})", pf.file, pf.pattern, pf.mode);
            }
            eprintln!("\nUse --allow-protected to override.");
        }
        // Return error with exit code 3 (policy blocked)
        return Err(Error::ProtectedPath(protected_guard[0].file.clone().into()));
    }

    // TODO: Inject Change-Id if missing (sv-8jf.5.2)

    // Check lease conflicts (sv-8jf.5.3)
    // Get current branch name for scope filtering
    let current_branch = repository.head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()));
    
    let lease_conflicts = check_lease_conflicts(
        &repository,
        &staged_files,
        current_branch.as_deref(),
    )?;
    
    if !lease_conflicts.is_empty() && !options.force_lease {
        // Print human-readable error (JSON errors are handled by main)
        if !options.json && !options.quiet {
            eprintln!("Error: Commit blocked by lease conflicts:");
            for conflict in &lease_conflicts {
                eprintln!("  {} is held by {} with {} strength (lease: {})",
                    conflict.file, conflict.holder, conflict.strength, conflict.lease_id);
            }
            eprintln!("\nUse --force-lease to override.");
        }
        // Return error with exit code 3 (policy blocked)
        return Err(Error::LeaseConflict {
            path: lease_conflicts[0].file.clone().into(),
            holder: lease_conflicts[0].holder.clone(),
            strength: lease_conflicts[0].strength.clone(),
        });
    }

    // Build git commit command
    let mut cmd = Command::new("git");
    cmd.arg("commit");
    cmd.current_dir(workdir);

    // Add message options
    if let Some(ref msg) = options.message {
        cmd.arg("-m").arg(msg);
    }
    if let Some(ref file) = options.file {
        cmd.arg("-F").arg(file);
    }
    if options.amend {
        cmd.arg("--amend");
    }
    if options.no_edit {
        cmd.arg("--no-edit");
    }

    // Execute git commit
    let output = cmd.output()
        .map_err(|e| Error::Io(e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        // Print human-readable error (JSON errors are handled by main)
        if !options.json && !options.quiet {
            eprint!("{}", stderr);
        }
        return Err(Error::OperationFailed(format!("git commit failed: {}", stderr.trim())));
    }

    // Extract commit hash from output or HEAD
    let commit_hash = get_head_commit_hash(&repository)?;

    if options.json {
        let result = CommitResult {
            success: true,
            commit_hash: Some(commit_hash),
            message: None,
            files_committed: staged_files,
            protected_files: vec![],
            lease_conflicts: vec![],
        };
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else if !options.quiet {
        print!("{}", stdout);
    }

    Ok(())
}

/// Check for protected path violations on files being committed
///
/// Returns two lists:
/// - guard mode files (should block commit)
/// - warn mode files (just emit warning)
fn check_protected_paths(
    repo_root: &PathBuf,
    staged_files: &[String],
) -> Result<(Vec<ProtectedFileInfo>, Vec<ProtectedFileInfo>)> {
    // Load config
    let config = Config::load_from_repo(repo_root);
    
    // Load per-workspace overrides
    let storage = Storage::for_repo(repo_root.clone());
    let override_data = protect::load_override(&storage).ok();
    
    // Convert staged files to PathBuf for the protect API
    let staged_paths: Vec<PathBuf> = staged_files.iter().map(PathBuf::from).collect();
    
    // Compute protection status
    let status = protect::compute_status(&config, override_data.as_ref(), &staged_paths)?;
    
    let mut guard_files = Vec::new();
    let mut warn_files = Vec::new();
    
    for rule_status in &status.rules {
        // Skip disabled patterns
        if rule_status.disabled {
            continue;
        }
        
        for matched_file in &rule_status.matched_files {
            let info = ProtectedFileInfo {
                file: matched_file.to_string_lossy().to_string(),
                pattern: rule_status.rule.pattern.clone(),
                mode: rule_status.rule.mode.clone(),
            };
            
            match rule_status.rule.mode.as_str() {
                "guard" => guard_files.push(info),
                "warn" => warn_files.push(info),
                // "readonly" would be handled at file system level, not here
                _ => guard_files.push(info), // Default to guard for unknown modes
            }
        }
    }
    
    Ok((guard_files, warn_files))
}

/// Check for lease conflicts on files being committed
///
/// Returns a list of conflicts where staged files are under active
/// exclusive/strong leases owned by OTHER actors.
fn check_lease_conflicts(
    repo: &git2::Repository,
    staged_files: &[String],
    current_branch: Option<&str>,
) -> Result<Vec<LeaseConflictInfo>> {
    let workdir = repo.workdir()
        .ok_or_else(|| Error::OperationFailed("no working directory".to_string()))?;
    
    // Get the common git dir (handles worktrees correctly)
    let git_dir = git::common_dir(repo);
    
    // Get current actor
    let current_actor = actor::resolve_actor(Some(workdir), None).ok();
    
    // Load lease store using proper paths for worktrees
    let storage = Storage::new(
        workdir.to_path_buf(),
        git_dir,
        workdir.to_path_buf(),
    );
    let existing_leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut store = LeaseStore::from_vec(existing_leases);
    
    // Expire stale leases
    store.expire_stale();
    
    let mut conflicts = Vec::new();
    
    for file in staged_files {
        // Find active leases that conflict with this file
        for lease in store.active() {
            // Skip own leases
            if let (Some(ref current), Some(ref lease_actor)) = (&current_actor, &lease.actor) {
                if current == lease_actor {
                    continue;
                }
            }
            
            // Check lease scope - skip if scope doesn't apply to current context
            match &lease.scope {
                LeaseScope::Repo => {
                    // Repo-wide leases always apply
                }
                LeaseScope::Branch(branch) => {
                    // Only conflict if we're on the same branch
                    if let Some(current) = current_branch {
                        if current != branch {
                            continue;
                        }
                    }
                }
                LeaseScope::Workspace(ws) => {
                    // Workspace-scoped leases - check if we're in that workspace
                    // For now, skip workspace-scoped leases from other workspaces
                    // (would need workspace detection to do this properly)
                    let _ = ws; // Suppress unused warning for now
                }
            }
            
            // Check if lease pathspec overlaps with file
            if !lease.pathspec_overlaps(file) {
                continue;
            }
            
            // Only block on exclusive or strong leases
            if lease.strength == LeaseStrength::Exclusive || lease.strength == LeaseStrength::Strong {
                conflicts.push(LeaseConflictInfo {
                    file: file.clone(),
                    lease_id: lease.id.to_string()[..8].to_string(),
                    holder: lease.actor.clone().unwrap_or_else(|| "(ownerless)".to_string()),
                    strength: lease.strength.to_string(),
                });
            }
        }
    }
    
    Ok(conflicts)
}

/// Stage all modified tracked files (equivalent to git add -u)
fn stage_all_modified(repo: &git2::Repository) -> Result<()> {
    let mut index = repo.index()?;
    
    index.update_all(["*"].iter(), None)?;
    
    index.write()?;
    
    Ok(())
}

/// Get list of staged files
fn get_staged_files(repo: &git2::Repository) -> Result<Vec<String>> {
    let head = match repo.head() {
        Ok(head) => Some(head.peel_to_tree()?),
        Err(_) => None, // Initial commit, no HEAD yet
    };
    
    let index = repo.index()?;
    
    let diff = repo.diff_tree_to_index(
        head.as_ref(),
        Some(&index),
        None,
    )?;
    
    let mut files = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path() {
                files.push(path.to_string_lossy().to_string());
            }
            true
        },
        None,
        None,
        None,
    )?;
    
    Ok(files)
}

/// Get the current HEAD commit hash
fn get_head_commit_hash(repo: &git2::Repository) -> Result<String> {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string()[..8].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, git2::Repository) {
        let temp = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        
        // Configure user for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        
        (temp, repo)
    }

    #[test]
    fn test_get_staged_files_empty() {
        let (_temp, repo) = setup_test_repo();
        let files = get_staged_files(&repo).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_get_staged_files_with_staged() {
        let (temp, repo) = setup_test_repo();
        
        // Create and stage a file
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();
        
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("test.txt")).unwrap();
        index.write().unwrap();
        
        let files = get_staged_files(&repo).unwrap();
        assert_eq!(files, vec!["test.txt"]);
    }
}
