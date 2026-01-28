//! Git repository discovery, worktree operations, and common queries.
//!
//! This module wraps libgit2 operations used across sv, including:
//! - Repository discovery and validation
//! - Worktree enumeration, creation, and removal
//! - HEAD and branch information

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use git2::{BranchType, ErrorCode, Oid, Repository, Sort};

use crate::error::{Error, Result};

/// Basic information about the current HEAD.
#[derive(Debug, Clone)]
pub struct HeadInfo {
    /// Commit pointed to by HEAD.
    pub oid: Oid,
    /// Full ref name (e.g., "refs/heads/main") when available.
    pub name: Option<String>,
    /// Shorthand name (e.g., "main") when available.
    pub shorthand: Option<String>,
    /// Whether HEAD is detached.
    pub is_detached: bool,
}

/// Discover a git repository from a starting path.
pub fn discover_repo(start: Option<&Path>) -> Result<Repository> {
    let start_path = match start {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()?,
    };

    Repository::discover(&start_path).map_err(|err| {
        if err.code() == ErrorCode::NotFound {
            Error::RepoNotFound(start_path)
        } else {
            Error::Git(err)
        }
    })
}

/// Open a repository and validate it is a non-bare checkout.
pub fn open_repo(start: Option<&Path>) -> Result<Repository> {
    let repo = discover_repo(start)?;
    if repo.is_bare() {
        return Err(Error::OperationFailed(
            "bare repositories are not supported".to_string(),
        ));
    }
    Ok(repo)
}

/// Return the repository workdir (root of the working tree).
pub fn workdir(repo: &Repository) -> Result<PathBuf> {
    repo.workdir()
        .map(|path| path.to_path_buf())
        .ok_or_else(|| Error::OperationFailed("repository has no working directory".to_string()))
}

/// Return information about HEAD (ref name, shorthand, and commit).
pub fn head_info(repo: &Repository) -> Result<HeadInfo> {
    let head = repo.head()?;
    let oid = head
        .target()
        .ok_or_else(|| Error::OperationFailed("HEAD has no target commit".to_string()))?;

    // Check if HEAD is detached by seeing if it's a symbolic reference
    let is_detached = !head.is_branch();

    Ok(HeadInfo {
        oid,
        name: head.name().map(|name| name.to_string()),
        shorthand: head.shorthand().map(|name| name.to_string()),
        is_detached,
    })
}

// =============================================================================
// Worktree Operations
// =============================================================================

/// Information about a Git worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Name of the worktree (for linked worktrees) or "main" for the main worktree.
    pub name: String,
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch checked out in this worktree (if any).
    pub branch: Option<String>,
    /// Whether this is the main worktree.
    pub is_main: bool,
    /// Whether the worktree is locked.
    pub is_locked: bool,
    /// Whether the worktree directory is missing (prunable).
    pub is_prunable: bool,
}

/// List all worktrees in the repository.
///
/// Returns information about both the main worktree and any linked worktrees.
pub fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>> {
    let mut worktrees = Vec::new();

    // Add main worktree
    if let Some(main_path) = repo.workdir() {
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from));

        worktrees.push(WorktreeInfo {
            name: "main".to_string(),
            path: main_path.to_path_buf(),
            branch,
            is_main: true,
            is_locked: false,
            is_prunable: false,
        });
    }

    // List linked worktrees using libgit2
    let worktree_names = repo.worktrees()?;
    for name in worktree_names.iter() {
        let name = match name {
            Some(n) => n,
            None => continue,
        };

        match repo.find_worktree(name) {
            Ok(wt) => {
                let path = wt.path().to_path_buf();
                // is_locked returns Result<WorktreeLockStatus, Error>
                // WorktreeLockStatus is Unlocked or Locked(Option<String>)
                let is_locked = wt
                    .is_locked()
                    .map(|status| !matches!(status, git2::WorktreeLockStatus::Unlocked))
                    .unwrap_or(false);
                let is_prunable = wt.is_prunable(None).unwrap_or(false);

                // Try to get the branch for this worktree
                let branch = get_worktree_branch(&path);

                worktrees.push(WorktreeInfo {
                    name: name.to_string(),
                    path,
                    branch,
                    is_main: false,
                    is_locked,
                    is_prunable,
                });
            }
            Err(_) => {
                // Worktree entry exists but can't be opened (maybe corrupted)
                continue;
            }
        }
    }

    Ok(worktrees)
}

/// Get the branch checked out in a worktree directory.
fn get_worktree_branch(worktree_path: &Path) -> Option<String> {
    // Open the worktree as a repository
    let repo = Repository::open(worktree_path).ok()?;
    let head = repo.head().ok()?;
    head.shorthand().map(String::from)
}

/// Create a new worktree.
///
/// # Arguments
/// * `repo` - The main repository
/// * `name` - Name for the worktree (used for the branch if not specified)
/// * `path` - Directory path for the new worktree
/// * `base_ref` - Reference to branch from (e.g., "main", "HEAD", commit SHA)
/// * `branch_name` - Optional branch name (defaults to `sv/ws/<name>`)
///
/// # Returns
/// The path to the created worktree.
pub fn create_worktree(
    repo: &Repository,
    name: &str,
    path: &Path,
    base_ref: &str,
    branch_name: Option<&str>,
) -> Result<PathBuf> {
    // Determine the branch name
    let branch = branch_name
        .map(String::from)
        .unwrap_or_else(|| format!("sv/ws/{}", name));

    // Resolve the base reference to a commit
    let base_commit = repo
        .revparse_single(base_ref)?
        .peel_to_commit()
        .map_err(|e| {
            Error::OperationFailed(format!("Cannot resolve '{}' to commit: {}", base_ref, e))
        })?;

    // Check if branch already exists
    if repo.find_branch(&branch, BranchType::Local).is_ok() {
        return Err(Error::OperationFailed(format!(
            "Branch '{}' already exists",
            branch
        )));
    }

    // Check if worktree path already exists and is not empty
    if path.exists() {
        let is_empty = path
            .read_dir()
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            return Err(Error::OperationFailed(format!(
                "Workspace (worktree) path already exists and is not empty: {}",
                path.display()
            )));
        }
    }

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create the worktree using git command (libgit2's worktree API is limited)
    // This is more reliable than using libgit2 directly for worktree creation
    // We use -b instead of -B to fail early if branch exists (we check above)
    let repo_path = repo.path();
    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            "--checkout",
            "-b",
            &branch,
            &path.to_string_lossy(),
            &base_commit.id().to_string(),
        ])
        .current_dir(repo_path.parent().unwrap_or(repo_path))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Clean up: delete the branch if git worktree add created it but failed
        // This can happen if worktree add partially succeeded before failing
        if let Ok(mut branch_ref) = repo.find_branch(&branch, BranchType::Local) {
            // Only delete if it's not checked out anywhere
            let _ = branch_ref.delete();
        }

        return Err(Error::OperationFailed(format!(
            "Failed to create workspace (worktree): {}",
            stderr.trim()
        )));
    }

    Ok(path.to_path_buf())
}

/// Remove a worktree.
///
/// # Arguments
/// * `repo` - The main repository
/// * `name` - Name of the worktree to remove
/// * `force` - If true, remove even with uncommitted changes
///
/// # Returns
/// The path that was removed.
pub fn remove_worktree(repo: &Repository, name: &str, force: bool) -> Result<PathBuf> {
    // Find the worktree
    let wt = repo
        .find_worktree(name)
        .map_err(|_| Error::WorkspaceNotFound(name.to_string()))?;

    let path = wt.path().to_path_buf();

    // Check if it's locked
    let is_locked = wt
        .is_locked()
        .map(|status| !matches!(status, git2::WorktreeLockStatus::Unlocked))
        .unwrap_or(false);
    if is_locked && !force {
        return Err(Error::OperationFailed(format!(
            "Workspace (worktree) '{}' is locked. Use --force to remove anyway.",
            name
        )));
    }

    // Check for uncommitted changes unless force
    if !force {
        if let Ok(wt_repo) = Repository::open(&path) {
            if has_uncommitted_changes(&wt_repo)? {
                return Err(Error::OperationFailed(format!(
                    "Workspace (worktree) '{}' has uncommitted changes. Use --force to remove anyway.",
                    name
                )));
            }
        }
    }

    // Remove using git command (more reliable than libgit2)
    // Note: git worktree remove expects a path, not a name
    let repo_path = repo.path();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let path_str = path.to_string_lossy();
    args.push(&path_str);

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_path.parent().unwrap_or(repo_path))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::OperationFailed(format!(
            "Failed to remove workspace (worktree): {}",
            stderr.trim()
        )));
    }

    Ok(path)
}

/// Check if a repository has uncommitted changes.
pub fn has_uncommitted_changes(repo: &Repository) -> Result<bool> {
    let statuses = repo.statuses(None)?;

    for entry in statuses.iter() {
        let status = entry.status();
        // Check for any changes that aren't ignored
        if !status.is_ignored() && !status.is_empty() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Prune stale worktree entries.
///
/// Removes worktree entries where the directory no longer exists.
pub fn prune_worktrees(repo: &Repository) -> Result<Vec<String>> {
    let repo_path = repo.path();
    let output = Command::new("git")
        .args(["worktree", "prune", "-v"])
        .current_dir(repo_path.parent().unwrap_or(repo_path))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::OperationFailed(format!(
            "Failed to prune workspaces (worktrees): {}",
            stderr.trim()
        )));
    }

    // Parse the verbose output to see what was pruned
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pruned: Vec<String> = stdout
        .lines()
        .filter(|line| line.contains("Removing"))
        .map(|line| line.to_string())
        .collect();

    Ok(pruned)
}

/// Get the path to the git common directory.
///
/// For worktrees, this returns the path to the main repository's .git directory.
/// For normal repositories, this returns the .git directory path.
pub fn common_dir(repo: &Repository) -> PathBuf {
    let git_dir = repo.path();
    let commondir_file = git_dir.join("commondir");

    if commondir_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&commondir_file) {
            let rel = content.trim();
            if !rel.is_empty() {
                return git_dir.join(rel);
            }
        }
    }

    git_dir.to_path_buf()
}

// =============================================================================
// Diff and File Status Operations
// =============================================================================

/// Status of a file in the working tree or index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// File is new (not in the previous tree)
    Added,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed (includes copy)
    Renamed,
    /// File type changed (e.g., file to symlink)
    TypeChanged,
    /// File is untracked
    Untracked,
    /// File is ignored
    Ignored,
    /// File has a conflict
    Conflicted,
}

/// Information about a changed file.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path to the file (relative to repo root)
    pub path: PathBuf,
    /// Status of the file
    pub status: FileStatus,
    /// Old path if the file was renamed
    pub old_path: Option<PathBuf>,
}

/// Get the list of changed files between two refs.
///
/// # Arguments
/// * `repo` - The repository
/// * `from_ref` - Starting reference (e.g., "main", "HEAD~5", commit SHA)
/// * `to_ref` - Ending reference (use "HEAD" for current state, or None for working tree)
///
/// # Returns
/// A list of file changes between the two references.
pub fn diff_files(
    repo: &Repository,
    from_ref: &str,
    to_ref: Option<&str>,
) -> Result<Vec<FileChange>> {
    let from_tree = repo
        .revparse_single(from_ref)?
        .peel_to_tree()
        .map_err(|e| {
            Error::OperationFailed(format!("Cannot resolve '{}' to tree: {}", from_ref, e))
        })?;

    // If to_ref is None, diff against the working tree (including staged changes)
    // Otherwise, diff between two tree references
    let diff = match to_ref {
        Some(ref_name) => {
            let to_tree = repo
                .revparse_single(ref_name)?
                .peel_to_tree()
                .map_err(|e| {
                    Error::OperationFailed(format!("Cannot resolve '{}' to tree: {}", ref_name, e))
                })?;
            repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?
        }
        None => {
            // Diff from_tree against working directory (includes staged + unstaged)
            repo.diff_tree_to_workdir_with_index(Some(&from_tree), None)?
        }
    };

    parse_diff_to_changes(&diff)
}

/// Get the list of files changed in the working tree compared to HEAD.
///
/// This includes both staged and unstaged changes.
pub fn working_tree_changes(repo: &Repository) -> Result<Vec<FileChange>> {
    let head_tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_workdir_with_index(Some(&head_tree), None)?;
    parse_diff_to_changes(&diff)
}

/// Get the list of staged files (files in the index that differ from HEAD).
pub fn staged_files(repo: &Repository) -> Result<Vec<FileChange>> {
    let head_tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_index(Some(&head_tree), Some(&repo.index()?), None)?;
    parse_diff_to_changes(&diff)
}

/// Get the list of unstaged changes (working tree vs index).
pub fn unstaged_files(repo: &Repository) -> Result<Vec<FileChange>> {
    let diff = repo.diff_index_to_workdir(None, None)?;
    parse_diff_to_changes(&diff)
}

/// Parse a git2 Diff into our FileChange structure.
fn parse_diff_to_changes(diff: &git2::Diff) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();

    for delta in diff.deltas() {
        let status = match delta.status() {
            git2::Delta::Added => FileStatus::Added,
            git2::Delta::Deleted => FileStatus::Deleted,
            git2::Delta::Modified => FileStatus::Modified,
            git2::Delta::Renamed => FileStatus::Renamed,
            git2::Delta::Copied => FileStatus::Renamed,
            git2::Delta::Typechange => FileStatus::TypeChanged,
            git2::Delta::Untracked => FileStatus::Untracked,
            git2::Delta::Ignored => FileStatus::Ignored,
            git2::Delta::Conflicted => FileStatus::Conflicted,
            _ => continue, // Skip unmodified and other statuses
        };

        // For renamed files, new_file has the current path
        let path = delta
            .new_file()
            .path()
            .map(PathBuf::from)
            .unwrap_or_default();

        // For renamed files, old_file has the original path
        let old_path = if status == FileStatus::Renamed {
            delta.old_file().path().map(PathBuf::from)
        } else {
            None
        };

        changes.push(FileChange {
            path,
            status,
            old_path,
        });
    }

    Ok(changes)
}

/// Get paths only from file changes (convenience function).
pub fn changed_paths(changes: &[FileChange]) -> Vec<PathBuf> {
    changes.iter().map(|c| c.path.clone()).collect()
}

/// Get staged file paths only.
pub fn staged_paths(repo: &Repository) -> Result<Vec<PathBuf>> {
    let changes = staged_files(repo)?;
    Ok(changed_paths(&changes))
}

/// Check if any of the given paths have uncommitted changes.
///
/// This checks both staged and unstaged changes.
pub fn has_changes_in_paths(repo: &Repository, paths: &[PathBuf]) -> Result<bool> {
    let changes = working_tree_changes(repo)?;

    for change in changes {
        if paths
            .iter()
            .any(|p| change.path == *p || change.path.starts_with(p) || p.starts_with(&change.path))
        {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Get file status using git status.
///
/// This provides comprehensive status including untracked files.
pub fn file_statuses(repo: &Repository) -> Result<Vec<FileChange>> {
    let statuses = repo.statuses(None)?;
    let mut changes = Vec::new();

    for entry in statuses.iter() {
        let path = entry.path().map(|p| PathBuf::from(p)).unwrap_or_default();

        let status = entry.status();

        // Determine the primary status
        let file_status = if status.is_conflicted() {
            FileStatus::Conflicted
        } else if status.is_ignored() {
            FileStatus::Ignored
        } else if status.is_wt_new() || status.is_index_new() {
            if status.is_wt_new() && !status.is_index_new() {
                FileStatus::Untracked
            } else {
                FileStatus::Added
            }
        } else if status.is_wt_deleted() || status.is_index_deleted() {
            FileStatus::Deleted
        } else if status.is_wt_renamed() || status.is_index_renamed() {
            FileStatus::Renamed
        } else if status.is_wt_typechange() || status.is_index_typechange() {
            FileStatus::TypeChanged
        } else if status.is_wt_modified() || status.is_index_modified() {
            FileStatus::Modified
        } else {
            continue; // Skip unchanged files
        };

        changes.push(FileChange {
            path,
            status: file_status,
            old_path: None, // git2 statuses don't provide old path for renames
        });
    }

    Ok(changes)
}

// =============================================================================
// Commit Operations
// =============================================================================

/// Result of creating a commit.
#[derive(Debug, Clone)]
pub struct CommitResult {
    /// The OID of the created commit.
    pub oid: Oid,
    /// The commit message used.
    pub message: String,
    /// Whether the message was modified (e.g., trailers added).
    pub message_modified: bool,
}

/// Options for creating a commit.
#[derive(Debug, Clone, Default)]
pub struct CommitOptions {
    /// If true, amend the current HEAD instead of creating a new commit.
    pub amend: bool,
    /// If true, allow creating an empty commit (no changes).
    pub allow_empty: bool,
    /// Author name (defaults to Git config).
    pub author_name: Option<String>,
    /// Author email (defaults to Git config).
    pub author_email: Option<String>,
}

/// Create a commit with the staged changes.
///
/// # Arguments
/// * `repo` - The repository
/// * `message` - Commit message
/// * `options` - Commit options (amend, allow_empty, etc.)
///
/// # Returns
/// The result containing the commit OID and final message.
pub fn create_commit(
    repo: &Repository,
    message: &str,
    options: &CommitOptions,
) -> Result<CommitResult> {
    // Get the index (staging area)
    let mut index = repo.index()?;

    // Write the index as a tree
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    // Get parent commits
    let parents = if options.amend {
        // For amend, we use the parents of HEAD
        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;
        head_commit.parents().collect::<Vec<_>>()
    } else {
        // For a normal commit, HEAD is the parent (if it exists)
        match repo.head() {
            Ok(head) => vec![head.peel_to_commit()?],
            Err(e) if e.code() == ErrorCode::UnbornBranch => vec![],
            Err(e) => return Err(Error::Git(e)),
        }
    };

    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

    // Check for empty commits
    if !options.allow_empty && !options.amend {
        if !parent_refs.is_empty() {
            let parent_tree = parent_refs[0].tree()?;
            let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
            if diff.deltas().count() == 0 {
                return Err(Error::OperationFailed(
                    "nothing to commit (use --allow-empty to create an empty commit)".to_string(),
                ));
            }
        }
    }

    // Get signature (author/committer)
    let signature = match (&options.author_name, &options.author_email) {
        (Some(name), Some(email)) => git2::Signature::now(name, email)?,
        _ => repo.signature()?,
    };

    // Create or amend the commit
    let oid = if options.amend {
        // For amend, we use git2's commit_amend to replace HEAD
        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;

        head_commit.amend(
            Some("HEAD"),
            None,             // Keep author
            Some(&signature), // Update committer
            None,             // Keep encoding
            Some(message),
            Some(&tree),
        )?
    } else {
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )?
    };

    Ok(CommitResult {
        oid,
        message: message.to_string(),
        message_modified: false,
    })
}

/// Amend the current HEAD commit with a new message.
///
/// This is a convenience wrapper around `create_commit` with `amend: true`.
pub fn amend_commit_message(repo: &Repository, message: &str) -> Result<CommitResult> {
    create_commit(
        repo,
        message,
        &CommitOptions {
            amend: true,
            ..Default::default()
        },
    )
}

/// Get the message of a commit.
pub fn get_commit_message(repo: &Repository, oid: Oid) -> Result<String> {
    let commit = repo.find_commit(oid)?;
    Ok(commit.message().unwrap_or_default().to_string())
}

/// Get the message of the HEAD commit.
pub fn head_commit_message(repo: &Repository) -> Result<String> {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.message().unwrap_or_default().to_string())
}

/// List commits reachable from `branch_ref` but not from `base_ref`.
///
/// Returns commits in topological/time order (newest first).
pub fn commits_ahead(repo: &Repository, base_ref: &str, branch_ref: &str) -> Result<Vec<Oid>> {
    let mut revwalk = repo.revwalk()?;
    let range = format!("{base_ref}..{branch_ref}");
    revwalk.push_range(&range).map_err(|err| {
        Error::OperationFailed(format!("unable to walk range '{}': {}", range, err))
    })?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let mut commits = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        commits.push(oid);
    }

    Ok(commits)
}

/// Check whether `ancestor_ref` is an ancestor of `descendant_ref`.
pub fn is_ancestor(repo: &Repository, ancestor_ref: &str, descendant_ref: &str) -> Result<bool> {
    let ancestor = repo.revparse_single(ancestor_ref)?.peel_to_commit()?.id();
    let descendant = repo.revparse_single(descendant_ref)?.peel_to_commit()?.id();
    if ancestor == descendant {
        return Ok(true);
    }
    repo.graph_descendant_of(descendant, ancestor)
        .map_err(Error::Git)
}

/// Compute a stable patch-id for a commit.
///
/// Uses `git patch-id --stable` to match Git's own dedup behavior.
pub fn patch_id(repo: &Repository, oid: Oid) -> Result<String> {
    let workdir = workdir(repo)?;
    let show = Command::new("git")
        .args(["show", "--pretty=format:", "--no-color", &oid.to_string()])
        .current_dir(&workdir)
        .output()?;

    if !show.status.success() {
        return Err(Error::OperationFailed(format!(
            "git show failed: {}",
            String::from_utf8_lossy(&show.stderr)
        )));
    }

    let mut child = Command::new("git")
        .args(["patch-id", "--stable"])
        .current_dir(&workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::OperationFailed("git patch-id stdin unavailable".to_string()))?;
        stdin.write_all(&show.stdout)?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(Error::OperationFailed(format!(
            "git patch-id failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let patch_id = stdout
        .split_whitespace()
        .next()
        .ok_or_else(|| Error::OperationFailed("git patch-id returned empty output".to_string()))?;

    Ok(patch_id.to_string())
}

// =============================================================================
// Trailer Operations
// =============================================================================

/// A parsed trailer from a commit message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    /// The key (e.g., "Change-Id", "Signed-off-by").
    pub key: String,
    /// The value.
    pub value: String,
}

impl Trailer {
    /// Create a new trailer.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

impl std::fmt::Display for Trailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.value)
    }
}

/// Parse trailers from a commit message.
///
/// Trailers are key-value pairs at the end of a commit message, separated by
/// a blank line from the body. Each trailer has the format `Key: Value`.
///
/// # Example
/// ```ignore
/// let msg = "Fix bug\n\nSome description\n\nChange-Id: abc\nSigned-off-by: Name <email>";
/// let trailers = parse_trailers(msg);
/// assert_eq!(trailers.len(), 2);
/// ```
pub fn parse_trailers(message: &str) -> Vec<Trailer> {
    let mut trailers = Vec::new();

    // Find the trailer block (lines at the end that match trailer format)
    let lines: Vec<&str> = message.lines().collect();

    // Walk backwards from the end to find trailers
    let mut trailer_start = lines.len();
    for (i, line) in lines.iter().enumerate().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Hit a blank line, trailer block ends here
            break;
        }
        if is_trailer_line(trimmed) {
            trailer_start = i;
        } else {
            // Not a trailer line, stop scanning
            break;
        }
    }

    // Parse the trailer lines
    for line in &lines[trailer_start..] {
        if let Some(trailer) = parse_trailer_line(line.trim()) {
            trailers.push(trailer);
        }
    }

    trailers
}

/// Check if a line looks like a trailer (Key: Value format).
fn is_trailer_line(line: &str) -> bool {
    if let Some(colon_pos) = line.find(':') {
        let key = &line[..colon_pos];
        // Key must be non-empty, contain only valid chars (alphanumeric, dash)
        !key.is_empty()
            && key.chars().all(|c| c.is_alphanumeric() || c == '-')
            && line.len() > colon_pos + 1 // Has value part
    } else {
        false
    }
}

/// Parse a single trailer line.
fn parse_trailer_line(line: &str) -> Option<Trailer> {
    let colon_pos = line.find(':')?;
    let key = line[..colon_pos].trim().to_string();
    let value = line[colon_pos + 1..].trim().to_string();

    if key.is_empty() || value.is_empty() {
        return None;
    }

    Some(Trailer { key, value })
}

/// Find a trailer by key (case-insensitive).
pub fn find_trailer<'a>(trailers: &'a [Trailer], key: &str) -> Option<&'a Trailer> {
    trailers.iter().find(|t| t.key.eq_ignore_ascii_case(key))
}

/// Add or update a trailer in a commit message.
///
/// If a trailer with the same key exists, it is updated. Otherwise, a new
/// trailer is added to the end of the trailer block.
///
/// Returns the modified message and whether it was changed.
pub fn set_trailer(message: &str, key: &str, value: &str) -> (String, bool) {
    let trailers = parse_trailers(message);
    let existing = find_trailer(&trailers, key);

    if let Some(t) = existing {
        if t.value == value {
            // Same value, no change needed
            return (message.to_string(), false);
        }
        // Replace existing trailer
        let old_line = format!("{}: {}", t.key, t.value);
        let new_line = format!("{}: {}", key, value);
        let updated = message.replace(&old_line, &new_line);
        return (updated, true);
    }

    // Add new trailer
    let updated = append_trailer(message, key, value);
    (updated, true)
}

/// Append a trailer to a commit message.
///
/// Ensures proper formatting with a blank line before the trailer block if needed.
fn append_trailer(message: &str, key: &str, value: &str) -> String {
    let trimmed = message.trim_end();
    let trailer_line = format!("{}: {}", key, value);

    // Check if there's already a trailer block
    let lines: Vec<&str> = trimmed.lines().collect();
    let has_trailer_block = lines
        .iter()
        .rev()
        .take_while(|l| !l.trim().is_empty())
        .any(|l| is_trailer_line(l.trim()));

    if has_trailer_block {
        // Append to existing trailer block
        format!("{}\n{}\n", trimmed, trailer_line)
    } else if trimmed.is_empty() {
        // Empty message, just add trailer
        format!("{}\n", trailer_line)
    } else {
        // No trailer block, add one with blank line separator
        format!("{}\n\n{}\n", trimmed, trailer_line)
    }
}

/// Remove a trailer by key from a commit message.
///
/// Returns the modified message and whether it was changed.
pub fn remove_trailer(message: &str, key: &str) -> (String, bool) {
    let trailers = parse_trailers(message);
    let existing = find_trailer(&trailers, key);

    if let Some(t) = existing {
        let trailer_line = format!("{}: {}", t.key, t.value);
        // Remove the line (including potential newline)
        let updated = message
            .replace(&format!("\n{}\n", trailer_line), "\n")
            .replace(&format!("{}\n", trailer_line), "");
        (updated.trim_end().to_string() + "\n", true)
    } else {
        (message.to_string(), false)
    }
}

/// Filter changes to only include files matching a pathspec.
pub fn filter_changes_by_pathspec(
    changes: Vec<FileChange>,
    pathspec: &[String],
) -> Vec<FileChange> {
    if pathspec.is_empty() {
        return changes;
    }

    changes
        .into_iter()
        .filter(|change| {
            let path_str = change.path.to_string_lossy();
            pathspec.iter().any(|pattern| {
                // Simple glob matching (could be enhanced with glob crate)
                if pattern.contains('*') {
                    glob_match(pattern, &path_str)
                } else {
                    path_str.starts_with(pattern) || path_str == *pattern
                }
            })
        })
        .collect()
}

/// Simple glob matching (handles * and ** patterns).
fn glob_match(pattern: &str, path: &str) -> bool {
    // Handle ** recursive patterns
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');

            // Check prefix matches
            if !prefix.is_empty() && !path.starts_with(prefix) {
                return false;
            }

            // Get the part of path after prefix
            let remaining = if prefix.is_empty() {
                path
            } else {
                path.strip_prefix(prefix)
                    .and_then(|p| p.strip_prefix('/'))
                    .unwrap_or(path)
            };

            // Check suffix (which may contain wildcards like *.rs)
            if suffix.is_empty() {
                return true;
            }

            // Handle suffix with wildcards
            if suffix.starts_with('*') {
                // *.rs pattern - match extension
                let ext = suffix.trim_start_matches('*');
                return path.ends_with(ext);
            }

            return remaining.ends_with(suffix) || path.ends_with(suffix);
        }
    }

    if pattern.ends_with("/*") {
        // dir/* matches direct children
        let dir = pattern.trim_end_matches("/*");
        if let Some(rest) = path.strip_prefix(dir) {
            if let Some(rest) = rest.strip_prefix('/') {
                return !rest.contains('/');
            }
        }
        return false;
    }

    if pattern.ends_with('*') {
        // prefix* matches paths starting with prefix
        let prefix = pattern.trim_end_matches('*');
        return path.starts_with(prefix);
    }

    if pattern.starts_with('*') {
        // *suffix matches paths ending with suffix
        let suffix = pattern.trim_start_matches('*');
        return path.ends_with(suffix);
    }

    // Exact match
    pattern == path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_test_repo() -> (TempDir, Repository) {
        let temp = TempDir::new().unwrap();

        // Initialize repo with git command
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        // Configure git user for commits
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(temp.path().join("README.md"), "# Test\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let repo = Repository::open(temp.path()).unwrap();
        (temp, repo)
    }

    #[test]
    fn test_list_worktrees_main_only() {
        let (_temp, repo) = init_test_repo();

        let worktrees = list_worktrees(&repo).unwrap();

        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_main);
        assert_eq!(worktrees[0].name, "main");
    }

    #[test]
    fn test_create_and_list_worktree() {
        let (temp, repo) = init_test_repo();

        let wt_path = temp.path().join(".sv").join("worktrees").join("test-wt");

        // Create worktree
        create_worktree(&repo, "test-wt", &wt_path, "HEAD", None).unwrap();

        // Verify it exists
        assert!(wt_path.exists());

        // List worktrees
        let worktrees = list_worktrees(&repo).unwrap();

        assert_eq!(worktrees.len(), 2);

        let linked = worktrees.iter().find(|w| !w.is_main).unwrap();
        assert_eq!(linked.name, "test-wt");
        assert_eq!(linked.branch, Some("sv/ws/test-wt".to_string()));
    }

    #[test]
    fn test_remove_worktree() {
        let (temp, repo) = init_test_repo();

        let wt_path = temp.path().join(".sv").join("worktrees").join("to-remove");

        // Create worktree
        create_worktree(&repo, "to-remove", &wt_path, "HEAD", None).unwrap();
        assert!(wt_path.exists());

        // Remove worktree
        remove_worktree(&repo, "to-remove", false).unwrap();

        // Verify it's gone
        assert!(!wt_path.exists());

        // Verify not in list
        let worktrees = list_worktrees(&repo).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_main);
    }

    #[test]
    fn test_common_dir() {
        let (_temp, repo) = init_test_repo();

        let cdir = common_dir(&repo);
        assert!(cdir.exists());
        assert!(cdir.ends_with(".git"));
    }

    #[test]
    fn test_staged_files() {
        let (temp, repo) = init_test_repo();

        // Create and stage a new file
        std::fs::write(temp.path().join("new_file.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "new_file.txt"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let staged = staged_files(&repo).unwrap();

        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].path, PathBuf::from("new_file.txt"));
        assert_eq!(staged[0].status, FileStatus::Added);
    }

    #[test]
    fn test_working_tree_changes() {
        let (temp, repo) = init_test_repo();

        // Modify an existing file (not staged)
        std::fs::write(temp.path().join("README.md"), "# Modified\n").unwrap();

        let changes = working_tree_changes(&repo).unwrap();

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, PathBuf::from("README.md"));
        assert_eq!(changes[0].status, FileStatus::Modified);
    }

    #[test]
    fn test_diff_files_between_commits() {
        let (temp, repo) = init_test_repo();

        // Get first commit
        let first_commit = repo.head().unwrap().target().unwrap().to_string();

        // Create a new file and commit
        std::fs::write(temp.path().join("new_file.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        // Diff between first commit and HEAD
        let changes = diff_files(&repo, &first_commit, Some("HEAD")).unwrap();

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, PathBuf::from("new_file.txt"));
        assert_eq!(changes[0].status, FileStatus::Added);
    }

    #[test]
    fn test_is_ancestor() {
        let (temp, repo) = init_test_repo();
        let base_branch = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(String::from))
            .unwrap_or_else(|| "master".to_string());

        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        std::fs::write(temp.path().join("feature.txt"), "feature\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "feature work"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        assert!(is_ancestor(&repo, &base_branch, "feature").unwrap());
        assert!(!is_ancestor(&repo, "feature", &base_branch).unwrap());
    }

    #[test]
    fn test_file_statuses_untracked() {
        let (temp, repo) = init_test_repo();

        // Create an untracked file
        std::fs::write(temp.path().join("untracked.txt"), "content").unwrap();

        let statuses = file_statuses(&repo).unwrap();

        let untracked = statuses
            .iter()
            .find(|s| s.path == PathBuf::from("untracked.txt"));
        assert!(untracked.is_some());
        assert_eq!(untracked.unwrap().status, FileStatus::Untracked);
    }

    #[test]
    fn test_glob_match() {
        // Test ** patterns
        assert!(glob_match("src/**/*.rs", "src/cli/mod.rs"));
        assert!(glob_match("src/**", "src/cli/mod.rs"));
        assert!(!glob_match("test/**", "src/cli/mod.rs"));

        // Test * suffix
        assert!(glob_match("*.rs", "mod.rs"));
        assert!(!glob_match("*.rs", "mod.txt"));

        // Test * prefix
        assert!(glob_match("src/*", "src/mod.rs"));

        // Test exact match
        assert!(glob_match("src/mod.rs", "src/mod.rs"));
        assert!(!glob_match("src/mod.rs", "src/lib.rs"));
    }

    #[test]
    fn test_filter_changes_by_pathspec() {
        let changes = vec![
            FileChange {
                path: PathBuf::from("src/main.rs"),
                status: FileStatus::Modified,
                old_path: None,
            },
            FileChange {
                path: PathBuf::from("src/lib.rs"),
                status: FileStatus::Modified,
                old_path: None,
            },
            FileChange {
                path: PathBuf::from("tests/test.rs"),
                status: FileStatus::Added,
                old_path: None,
            },
        ];

        // Filter to only src/**
        let filtered = filter_changes_by_pathspec(changes.clone(), &["src/**".to_string()]);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|c| c.path.starts_with("src")));

        // Filter to specific file
        let filtered2 = filter_changes_by_pathspec(changes, &["src/main.rs".to_string()]);

        assert_eq!(filtered2.len(), 1);
        assert_eq!(filtered2[0].path, PathBuf::from("src/main.rs"));
    }

    // ==========================================================================
    // Commit Operation Tests
    // ==========================================================================

    #[test]
    fn test_create_commit() {
        let (temp, repo) = init_test_repo();

        // Create and stage a new file
        std::fs::write(temp.path().join("new_file.txt"), "content").unwrap();
        Command::new("git")
            .args(["add", "new_file.txt"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let result = create_commit(&repo, "Add new file", &CommitOptions::default()).unwrap();

        assert!(!result.oid.is_zero());
        assert_eq!(result.message, "Add new file");

        // Verify the commit exists
        let commit = repo.find_commit(result.oid).unwrap();
        assert!(commit.message().unwrap().contains("Add new file"));
    }

    #[test]
    fn test_create_commit_empty_fails() {
        let (_temp, repo) = init_test_repo();

        // Try to create a commit without staging anything
        let result = create_commit(&repo, "Empty commit", &CommitOptions::default());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nothing to commit"));
    }

    #[test]
    fn test_create_commit_allow_empty() {
        let (_temp, repo) = init_test_repo();

        let opts = CommitOptions {
            allow_empty: true,
            ..Default::default()
        };
        let result = create_commit(&repo, "Empty commit allowed", &opts).unwrap();

        assert!(!result.oid.is_zero());
    }

    #[test]
    fn test_amend_commit() {
        let (_temp, repo) = init_test_repo();

        // Get original HEAD
        let original_head = repo.head().unwrap().target().unwrap();

        // Amend the last commit
        let result = amend_commit_message(&repo, "Amended commit message").unwrap();

        assert!(!result.oid.is_zero());
        assert_ne!(result.oid, original_head); // OID changed

        // Verify the message was updated
        let commit = repo.find_commit(result.oid).unwrap();
        assert!(commit.message().unwrap().contains("Amended commit message"));
    }

    #[test]
    fn test_head_commit_message() {
        let (_temp, repo) = init_test_repo();

        let msg = head_commit_message(&repo).unwrap();
        assert!(msg.contains("Initial commit"));
    }

    #[test]
    fn test_commits_ahead() {
        let (temp, repo) = init_test_repo();
        let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();

        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        std::fs::write(temp.path().join("change.txt"), "one").unwrap();
        Command::new("git")
            .args(["add", "change.txt"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "First change"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        let first = repo.head().unwrap().target().unwrap();

        std::fs::write(temp.path().join("change.txt"), "two").unwrap();
        Command::new("git")
            .args(["add", "change.txt"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Second change"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        let second = repo.head().unwrap().target().unwrap();

        let commits = commits_ahead(&repo, &base_branch, "feature").unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0], second);
        assert_eq!(commits[1], first);
    }

    // ==========================================================================
    // Trailer Operation Tests
    // ==========================================================================

    #[test]
    fn test_parse_trailers_single() {
        let msg = "Fix bug\n\nChange-Id: abc123\n";
        let trailers = parse_trailers(msg);

        assert_eq!(trailers.len(), 1);
        assert_eq!(trailers[0].key, "Change-Id");
        assert_eq!(trailers[0].value, "abc123");
    }

    #[test]
    fn test_parse_trailers_multiple() {
        let msg =
            "Fix bug\n\nSome description\n\nChange-Id: abc\nSigned-off-by: Test <test@test.com>\n";
        let trailers = parse_trailers(msg);

        assert_eq!(trailers.len(), 2);
        assert_eq!(trailers[0].key, "Change-Id");
        assert_eq!(trailers[0].value, "abc");
        assert_eq!(trailers[1].key, "Signed-off-by");
        assert_eq!(trailers[1].value, "Test <test@test.com>");
    }

    #[test]
    fn test_parse_trailers_none() {
        let msg = "Simple commit message\n";
        let trailers = parse_trailers(msg);

        assert!(trailers.is_empty());
    }

    #[test]
    fn test_find_trailer() {
        let trailers = vec![
            Trailer::new("Change-Id", "abc"),
            Trailer::new("Signed-off-by", "Test"),
        ];

        // Case-insensitive search
        assert!(find_trailer(&trailers, "change-id").is_some());
        assert!(find_trailer(&trailers, "CHANGE-ID").is_some());
        assert!(find_trailer(&trailers, "Change-Id").is_some());
        assert!(find_trailer(&trailers, "Not-Found").is_none());
    }

    #[test]
    fn test_set_trailer_new() {
        let msg = "Fix bug\n";
        let (updated, changed) = set_trailer(msg, "Change-Id", "abc123");

        assert!(changed);
        assert!(updated.contains("Change-Id: abc123"));
        // Should have blank line before trailer block
        assert!(updated.contains("\n\nChange-Id:"));
    }

    #[test]
    fn test_set_trailer_update() {
        let msg = "Fix bug\n\nChange-Id: old-value\n";
        let (updated, changed) = set_trailer(msg, "Change-Id", "new-value");

        assert!(changed);
        assert!(updated.contains("Change-Id: new-value"));
        assert!(!updated.contains("old-value"));
    }

    #[test]
    fn test_set_trailer_no_change() {
        let msg = "Fix bug\n\nChange-Id: same-value\n";
        let (updated, changed) = set_trailer(msg, "Change-Id", "same-value");

        assert!(!changed);
        assert_eq!(updated, msg);
    }

    #[test]
    fn test_set_trailer_append_to_existing_block() {
        let msg = "Fix bug\n\nChange-Id: abc\n";
        let (updated, changed) = set_trailer(msg, "Signed-off-by", "Test");

        assert!(changed);
        assert!(updated.contains("Change-Id: abc"));
        assert!(updated.contains("Signed-off-by: Test"));
    }

    #[test]
    fn test_remove_trailer() {
        let msg = "Fix bug\n\nChange-Id: abc\nSigned-off-by: Test\n";
        let (updated, changed) = remove_trailer(msg, "Change-Id");

        assert!(changed);
        assert!(!updated.contains("Change-Id"));
        assert!(updated.contains("Signed-off-by: Test"));
    }

    #[test]
    fn test_remove_trailer_not_found() {
        let msg = "Fix bug\n\nChange-Id: abc\n";
        let (updated, changed) = remove_trailer(msg, "Not-Found");

        assert!(!changed);
        assert_eq!(updated, msg);
    }

    #[test]
    fn test_trailer_display() {
        let trailer = Trailer::new("Change-Id", "abc123");
        assert_eq!(format!("{}", trailer), "Change-Id: abc123");
    }
}
