//! Storage layer for sv
//!
//! Manages persistent state in two locations:
//! - `.sv/` - Workspace-local state (per-worktree, ignored)
//! - `.git/sv/` - Shared local state (per-clone, ignored)
//!
//! # Directory Structure
//!
//! ```text
//! .sv/                          # Workspace-local (ignored)
//!   actor                       # Current actor identity
//!   workspace.json              # Workspace metadata
//!   overrides/                  # Per-workspace config overrides
//!     protect.json              # Protected paths disabled for this workspace
//!
//! .git/sv/                      # Shared local (per-clone, ignored)
//!   workspaces.json             # Registry of all workspaces
//!   leases.jsonl                # Active and historical leases
//!   oplog/                      # Operation log entries
//!     <timestamp>-<uuid>.json   # Individual operation records
//! ```

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::lock::{self, FileLock, DEFAULT_LOCK_TIMEOUT_MS};

/// Name of the workspace-local directory
pub const LOCAL_DIR: &str = ".sv";

/// Name of the shared directory within .git
pub const SHARED_DIR: &str = "sv";

/// Storage manager for sv state
#[derive(Debug, Clone)]
pub struct Storage {
    /// Path to the repository root (where .git lives)
    #[allow(dead_code)]
    repo_root: PathBuf,
    /// Path to .git directory (or worktree's gitdir)
    git_dir: PathBuf,
    /// Path to the workspace root (may differ from repo_root in worktrees)
    workspace_root: PathBuf,
}

impl Storage {
    /// Create a new storage manager for the given repository
    ///
    /// # Arguments
    /// * `repo_root` - Path to the repository root
    /// * `git_dir` - Path to the .git directory (handles worktrees)
    /// * `workspace_root` - Path to the workspace (worktree) root
    pub fn new(repo_root: PathBuf, git_dir: PathBuf, workspace_root: PathBuf) -> Self {
        Self {
            repo_root,
            git_dir,
            workspace_root,
        }
    }

    /// Create storage for a simple (non-worktree) repository
    pub fn for_repo(repo_root: PathBuf) -> Self {
        let git_dir = repo_root.join(".git");
        Self::new(repo_root.clone(), git_dir, repo_root)
    }

    // =========================================================================
    // Path accessors
    // =========================================================================

    /// Path to the workspace-local `.sv/` directory
    pub fn local_dir(&self) -> PathBuf {
        self.workspace_root.join(LOCAL_DIR)
    }

    /// Path to the workspace root directory
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Path to the shared `.git/sv/` directory
    pub fn shared_dir(&self) -> PathBuf {
        self.git_dir.join(SHARED_DIR)
    }

    /// Path to the actor file in workspace-local storage
    pub fn actor_file(&self) -> PathBuf {
        self.local_dir().join("actor")
    }

    /// Path to workspace metadata in local storage
    pub fn workspace_metadata_file(&self) -> PathBuf {
        self.local_dir().join("workspace.json")
    }

    /// Path to the per-workspace overrides directory
    pub fn overrides_dir(&self) -> PathBuf {
        self.local_dir().join("overrides")
    }

    /// Path to protected paths override file
    pub fn protect_override_file(&self) -> PathBuf {
        self.overrides_dir().join("protect.json")
    }

    /// Path to the workspaces registry
    pub fn workspaces_file(&self) -> PathBuf {
        self.shared_dir().join("workspaces.json")
    }

    /// Path to the leases file (JSONL format)
    pub fn leases_file(&self) -> PathBuf {
        self.shared_dir().join("leases.jsonl")
    }

    /// Path to the conflicts file (JSONL format)
    pub fn conflicts_file(&self) -> PathBuf {
        self.shared_dir().join("conflicts.jsonl")
    }

    /// Path to the operation log directory
    pub fn oplog_dir(&self) -> PathBuf {
        self.shared_dir().join("oplog")
    }

    /// Path to the hoist state directory
    pub fn hoist_dir(&self) -> PathBuf {
        self.shared_dir().join("hoist")
    }

    /// Path to the hoist state directory for a destination ref
    pub fn hoist_state_dir(&self, dest_ref: &str) -> PathBuf {
        self.hoist_dir().join(hoist_key(dest_ref))
    }

    /// Path to the hoist state file for a destination ref
    pub fn hoist_state_file(&self, dest_ref: &str) -> PathBuf {
        self.hoist_state_dir(dest_ref).join("state.json")
    }

    /// Path to the hoist conflicts file for a destination ref
    pub fn hoist_conflicts_file(&self, dest_ref: &str) -> PathBuf {
        self.hoist_state_dir(dest_ref).join("conflicts.jsonl")
    }

    // =========================================================================
    // Directory initialization
    // =========================================================================

    /// Initialize the workspace-local `.sv/` directory structure
    pub fn init_local(&self) -> Result<()> {
        let local = self.local_dir();

        // Create main directory
        fs::create_dir_all(&local)?;

        // Create overrides subdirectory
        fs::create_dir_all(self.overrides_dir())?;

        Ok(())
    }

    /// Initialize the shared `.git/sv/` directory structure
    pub fn init_shared(&self) -> Result<()> {
        let shared = self.shared_dir();

        // Create main directory
        fs::create_dir_all(&shared)?;

        // Create oplog subdirectory
        fs::create_dir_all(self.oplog_dir())?;

        // Create hoist state subdirectory
        fs::create_dir_all(self.hoist_dir())?;

        // Initialize empty workspaces registry if it doesn't exist
        let workspaces_file = self.workspaces_file();
        if !workspaces_file.exists() {
            self.write_json(&workspaces_file, &WorkspacesRegistry::default())?;
        }

        // Touch leases file if it doesn't exist
        let leases_file = self.leases_file();
        if !leases_file.exists() {
            File::create(&leases_file)?;
        }

        Ok(())
    }

    /// Initialize all storage directories
    pub fn init_all(&self) -> Result<()> {
        self.init_local()?;
        self.init_shared()?;
        Ok(())
    }

    /// Check if storage has been initialized
    pub fn is_initialized(&self) -> bool {
        self.shared_dir().exists()
    }

    // =========================================================================
    // File I/O helpers (atomic writes for safety)
    // =========================================================================

    /// Write JSON data atomically (write to temp, then rename)
    ///
    /// This ensures that concurrent readers never see partial writes.
    pub fn write_json<T: Serialize>(&self, path: &Path, data: &T) -> Result<()> {
        let json = serde_json::to_string_pretty(data)?;
        self.write_atomic(path, json.as_bytes())
    }

    /// Read JSON data from a file
    pub fn read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<T> {
        let content = fs::read_to_string(path)?;
        let data: T = serde_json::from_str(&content)?;
        Ok(data)
    }

    /// Write data atomically using temp file + rename
    ///
    /// This is critical for multi-agent safety: ensures readers never see
    /// partial writes, and the file is either fully written or not at all.
    pub fn write_atomic(&self, path: &Path, data: &[u8]) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create temp file in same directory (for atomic rename)
        let temp_path = path.with_extension("tmp");

        // Write to temp file
        let mut file = File::create(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?; // Ensure data is flushed to disk

        // Atomic rename
        fs::rename(&temp_path, path)?;

        Ok(())
    }

    /// Append a line to a JSONL file (for leases, oplog, etc.)
    ///
    /// Note: This is NOT atomic. For true concurrent safety, use file locking
    /// (implemented in the `locking` module). This method is for single-process
    /// use or when the caller holds a lock.
    pub fn append_jsonl<T: Serialize>(&self, path: &Path, record: &T) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string(record)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        writeln!(file, "{}", json)?;
        file.sync_all()?;

        Ok(())
    }

    /// Read all records from a JSONL file
    pub fn read_jsonl<T: DeserializeOwned>(&self, path: &Path) -> Result<Vec<T>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: T = serde_json::from_str(&line)?;
            records.push(record);
        }

        Ok(records)
    }

    // =========================================================================
    // Actor persistence
    // =========================================================================

    /// Read the persisted actor identity for this workspace
    pub fn read_actor(&self) -> Option<String> {
        let path = self.actor_file();
        fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
    }

    /// Write the actor identity for this workspace
    pub fn write_actor(&self, actor: &str) -> Result<()> {
        self.init_local()?; // Ensure directory exists
        let path = self.actor_file();
        self.write_atomic(&path, actor.as_bytes())
    }

    // =========================================================================
    // Workspace registry operations
    // =========================================================================

    /// Read the workspaces registry
    pub fn read_workspaces(&self) -> Result<WorkspacesRegistry> {
        let path = self.workspaces_file();
        if !path.exists() {
            return Ok(WorkspacesRegistry::default());
        }
        self.read_json(&path)
    }

    /// Write the workspaces registry (atomic)
    pub fn write_workspaces(&self, registry: &WorkspacesRegistry) -> Result<()> {
        let path = self.workspaces_file();
        self.write_json(&path, registry)
    }

    // =========================================================================
    // Workspace registry CRUD (locked)
    // =========================================================================

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceEntry>> {
        self.update_workspaces(|registry| Ok(registry.workspaces.clone()))
    }

    pub fn find_workspace(&self, name: &str) -> Result<Option<WorkspaceEntry>> {
        self.update_workspaces(|registry| Ok(registry.find(name).cloned()))
    }

    pub fn add_workspace(&self, entry: WorkspaceEntry) -> Result<()> {
        self.update_workspaces(|registry| registry.insert(entry))
    }

    pub fn update_workspace<F>(&self, name: &str, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut WorkspaceEntry) -> Result<()>,
    {
        self.update_workspaces(|registry| {
            let entry = registry
                .find_mut(name)
                .ok_or_else(|| Error::WorkspaceNotFound(name.to_string()))?;
            mutator(entry)?;
            entry.ensure_id();
            if !entry.path.exists() {
                return Err(Error::InvalidArgument(format!(
                    "workspace path does not exist: {}",
                    entry.path.display()
                )));
            }
            Ok(())
        })
    }

    pub fn remove_workspace(&self, name: &str) -> Result<Option<WorkspaceEntry>> {
        self.update_workspaces(|registry| Ok(registry.remove(name)))
    }

    pub fn cleanup_stale_workspaces(&self) -> Result<usize> {
        let path = self.workspaces_file();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_path = workspaces_lock_path(&path);
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

        let mut registry = if path.exists() {
            self.read_json(&path)?
        } else {
            WorkspacesRegistry::default()
        };

        let removed = registry.cleanup_stale();
        registry.validate()?;

        let json = serde_json::to_string_pretty(&registry)?;
        lock::write_atomic(&path, json.as_bytes())?;

        Ok(removed)
    }

    fn update_workspaces<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut WorkspacesRegistry) -> Result<T>,
    {
        let path = self.workspaces_file();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_path = workspaces_lock_path(&path);
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

        let mut registry = if path.exists() {
            self.read_json(&path)?
        } else {
            WorkspacesRegistry::default()
        };

        registry.cleanup_stale();
        let result = f(&mut registry)?;
        registry.validate()?;

        let json = serde_json::to_string_pretty(&registry)?;
        lock::write_atomic(&path, json.as_bytes())?;

        Ok(result)
    }

    // =========================================================================
    // Lease operations
    // =========================================================================

    /// Load all leases from the leases file into a LeaseStore
    pub fn load_leases(&self) -> Result<crate::lease::LeaseStore> {
        let leases: Vec<crate::lease::Lease> = self.read_jsonl(&self.leases_file())?;
        Ok(crate::lease::LeaseStore::from_vec(leases))
    }

    /// Save all leases to the leases file (overwrites)
    pub fn save_leases(&self, store: &crate::lease::LeaseStore) -> Result<()> {
        let path = self.leases_file();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write to temp file first
        let temp_path = path.with_extension("tmp");
        let mut file = File::create(&temp_path)?;

        for lease in store.all() {
            let json = serde_json::to_string(lease)?;
            writeln!(file, "{}", json)?;
        }

        file.sync_all()?;

        // Atomic rename
        fs::rename(&temp_path, &path)?;

        Ok(())
    }

    // =========================================================================
    // Conflict tracking operations
    // =========================================================================

    /// Load all conflict records from the conflicts file into a ConflictStore
    pub fn load_conflicts(&self) -> Result<crate::conflict::ConflictStore> {
        let records: Vec<crate::conflict::ConflictRecord> =
            self.read_jsonl(&self.conflicts_file())?;
        Ok(crate::conflict::ConflictStore::from_vec(records))
    }

    /// Append a conflict record to the conflicts file
    pub fn append_conflict(&self, record: &crate::conflict::ConflictRecord) -> Result<()> {
        self.append_jsonl(&self.conflicts_file(), record)
    }

    /// Save all conflicts to the conflicts file (overwrites)
    pub fn save_conflicts(&self, store: &crate::conflict::ConflictStore) -> Result<()> {
        let path = self.conflicts_file();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write to temp file first
        let temp_path = path.with_extension("tmp");
        let mut file = File::create(&temp_path)?;

        for record in store.all() {
            let json = serde_json::to_string(record)?;
            writeln!(file, "{}", json)?;
        }

        file.sync_all()?;

        // Atomic rename
        fs::rename(&temp_path, &path)?;

        Ok(())
    }

    /// Get all unresolved conflicts
    pub fn unresolved_conflicts(&self) -> Result<Vec<crate::conflict::ConflictRecord>> {
        let store = self.load_conflicts()?;
        Ok(store.unresolved().cloned().collect())
    }

    /// Mark a conflict as resolved by commit ID
    pub fn mark_conflict_resolved(&self, commit_id: &str) -> Result<bool> {
        let mut store = self.load_conflicts()?;
        let found = store.mark_resolved(commit_id);
        if found {
            self.save_conflicts(&store)?;
        }
        Ok(found)
    }

    // =========================================================================
    // Hoist state operations
    // =========================================================================

    /// Read hoist state for a destination ref.
    pub fn read_hoist_state(&self, dest_ref: &str) -> Result<Option<HoistState>> {
        let path = self.hoist_state_file(dest_ref);
        if !path.exists() {
            return Ok(None);
        }

        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        let state = self.read_json(&path)?;
        Ok(Some(state))
    }

    /// Write hoist state for a destination ref.
    pub fn write_hoist_state(&self, state: &HoistState) -> Result<()> {
        let path = self.hoist_state_file(&state.dest_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

        let json = serde_json::to_string_pretty(state)?;
        lock::write_atomic(&path, json.as_bytes())?;
        Ok(())
    }

    /// Append a hoist conflict record for a destination ref.
    pub fn append_hoist_conflict(&self, dest_ref: &str, record: &HoistConflict) -> Result<()> {
        let path = self.hoist_conflicts_file(dest_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        self.append_jsonl(&path, record)?;
        Ok(())
    }

    /// Read all hoist conflict records for a destination ref.
    pub fn read_hoist_conflicts(&self, dest_ref: &str) -> Result<Vec<HoistConflict>> {
        let path = self.hoist_conflicts_file(dest_ref);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        self.read_jsonl(&path)
    }

    /// Clear hoist state and conflicts for a destination ref.
    pub fn clear_hoist_state(&self, dest_ref: &str) -> Result<()> {
        let state_path = self.hoist_state_file(dest_ref);
        let conflicts_path = self.hoist_conflicts_file(dest_ref);

        if state_path.exists() {
            let lock_path = state_path.with_extension("lock");
            let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
            let _ = fs::remove_file(&state_path);
        }

        if conflicts_path.exists() {
            let lock_path = conflicts_path.with_extension("lock");
            let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
            let _ = fs::remove_file(&conflicts_path);
        }

        Ok(())
    }
}

// =============================================================================
// Data structures for registry files
// =============================================================================

/// Registry of all workspaces in this clone
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
pub struct WorkspacesRegistry {
    /// List of registered workspaces
    pub workspaces: Vec<WorkspaceEntry>,
}

/// Entry for a single workspace in the registry
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WorkspaceEntry {
    /// Unique workspace id
    #[serde(default = "default_workspace_id")]
    pub id: String,
    /// Unique workspace name
    pub name: String,
    /// Path to the workspace directory (absolute)
    pub path: PathBuf,
    /// Git branch associated with this workspace
    pub branch: String,
    /// Base ref this workspace was created from
    pub base: String,
    /// Actor currently associated with this workspace (if any)
    pub actor: Option<String>,
    /// Timestamp when workspace was created
    pub created_at: String,
    /// Timestamp of last activity
    pub last_active: Option<String>,
}

impl WorkspacesRegistry {
    /// Find a workspace by name
    pub fn find(&self, name: &str) -> Option<&WorkspaceEntry> {
        self.workspaces.iter().find(|w| w.name == name)
    }

    /// Find a workspace by name (mutable)
    pub fn find_mut(&mut self, name: &str) -> Option<&mut WorkspaceEntry> {
        self.workspaces.iter_mut().find(|w| w.name == name)
    }

    /// Insert a workspace entry (reject duplicates, require existing path)
    pub fn insert(&mut self, mut entry: WorkspaceEntry) -> Result<()> {
        entry.ensure_id();

        if self.find(&entry.name).is_some() {
            return Err(Error::InvalidArgument(format!(
                "workspace already exists: {}",
                entry.name
            )));
        }

        if !entry.path.exists() {
            return Err(Error::InvalidArgument(format!(
                "workspace path does not exist: {}",
                entry.path.display()
            )));
        }

        self.workspaces.push(entry);
        Ok(())
    }

    /// Remove a workspace by name
    pub fn remove(&mut self, name: &str) -> Option<WorkspaceEntry> {
        if let Some(idx) = self.workspaces.iter().position(|w| w.name == name) {
            Some(self.workspaces.remove(idx))
        } else {
            None
        }
    }

    /// Remove workspaces whose paths no longer exist
    pub fn cleanup_stale(&mut self) -> usize {
        let before = self.workspaces.len();
        self.workspaces.retain(|entry| entry.path.exists());
        before - self.workspaces.len()
    }

    /// Validate registry entries (unique names, existing paths)
    pub fn validate(&self) -> Result<()> {
        let mut names = HashSet::new();
        for entry in &self.workspaces {
            if !names.insert(entry.name.clone()) {
                return Err(Error::InvalidArgument(format!(
                    "duplicate workspace name: {}",
                    entry.name
                )));
            }
            if !entry.path.exists() {
                return Err(Error::InvalidArgument(format!(
                    "workspace path does not exist: {}",
                    entry.path.display()
                )));
            }
        }
        Ok(())
    }
}

/// Per-workspace override for protected paths
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
pub struct ProtectOverride {
    /// Patterns disabled for this workspace
    pub disabled_patterns: Vec<String>,
}

/// Persisted hoist state for a destination ref.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct HoistState {
    pub hoist_id: String,
    pub dest_ref: String,
    pub integration_ref: String,
    pub status: HoistStatus,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<HoistCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoistStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct HoistCommit {
    pub commit_id: String,
    pub status: HoistCommitStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoistCommitStatus {
    Pending,
    Applied,
    Skipped,
    Conflict,
    /// Commit was applied but contains unresolved conflict markers (jj-style)
    InConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct HoistConflict {
    pub hoist_id: String,
    pub commit_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

fn default_workspace_id() -> String {
    Uuid::new_v4().to_string()
}

impl WorkspaceEntry {
    pub fn new(
        name: String,
        path: PathBuf,
        branch: String,
        base: String,
        actor: Option<String>,
        created_at: String,
        last_active: Option<String>,
    ) -> Self {
        Self {
            id: default_workspace_id(),
            name,
            path,
            branch,
            base,
            actor,
            created_at,
            last_active,
        }
    }

    fn ensure_id(&mut self) {
        if self.id.trim().is_empty() {
            self.id = default_workspace_id();
        }
    }
}

fn workspaces_lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.display()))
}

fn hoist_key(dest_ref: &str) -> String {
    let mut key = String::new();
    for ch in dest_ref.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            key.push(ch);
        } else {
            key.push('_');
        }
    }
    if key.is_empty() {
        "_".to_string()
    } else {
        key
    }
}

// =============================================================================
// Utility functions
// =============================================================================

/// Ensure .sv/ is in .gitignore if not already present
pub fn ensure_gitignore(repo_root: &Path) -> io::Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    let sv_pattern = format!("/{}/", LOCAL_DIR);

    // Read existing .gitignore if it exists
    let existing = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    // Check if .sv/ is already ignored
    let already_ignored = existing.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == ".sv" || trimmed == ".sv/" || trimmed == "/.sv" || trimmed == "/.sv/"
    });

    if !already_ignored {
        // Append to .gitignore
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)?;

        // Add newline if file doesn't end with one
        if !existing.is_empty() && !existing.ends_with('\n') {
            writeln!(file)?;
        }

        writeln!(file, "# sv workspace-local state")?;
        writeln!(file, "{}", sv_pattern)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_paths() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();
        let storage = Storage::for_repo(repo_root.clone());

        assert_eq!(storage.local_dir(), repo_root.join(".sv"));
        assert_eq!(storage.shared_dir(), repo_root.join(".git/sv"));
        assert_eq!(
            storage.workspaces_file(),
            repo_root.join(".git/sv/workspaces.json")
        );
        assert_eq!(
            storage.leases_file(),
            repo_root.join(".git/sv/leases.jsonl")
        );
        assert_eq!(storage.oplog_dir(), repo_root.join(".git/sv/oplog"));
        assert_eq!(storage.hoist_dir(), repo_root.join(".git/sv/hoist"));
    }

    #[test]
    fn test_init_directories() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();

        // Create a fake .git directory
        fs::create_dir(repo_root.join(".git")).unwrap();

        let storage = Storage::for_repo(repo_root.clone());
        storage.init_all().unwrap();

        // Check directories exist
        assert!(storage.local_dir().exists());
        assert!(storage.shared_dir().exists());
        assert!(storage.oplog_dir().exists());
        assert!(storage.hoist_dir().exists());
        assert!(storage.overrides_dir().exists());

        // Check files exist
        assert!(storage.workspaces_file().exists());
        assert!(storage.leases_file().exists());
    }

    #[test]
    fn test_atomic_write() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();
        fs::create_dir(repo_root.join(".git")).unwrap();

        let storage = Storage::for_repo(repo_root);
        storage.init_all().unwrap();

        let test_file = storage.shared_dir().join("test.json");

        #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        storage.write_json(&test_file, &data).unwrap();
        let read_back: TestData = storage.read_json(&test_file).unwrap();

        assert_eq!(data, read_back);
    }

    #[test]
    fn test_jsonl_operations() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();
        fs::create_dir(repo_root.join(".git")).unwrap();

        let storage = Storage::for_repo(repo_root);
        storage.init_all().unwrap();

        #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Record {
            id: u32,
            message: String,
        }

        let file = storage.shared_dir().join("test.jsonl");

        // Append some records
        storage
            .append_jsonl(
                &file,
                &Record {
                    id: 1,
                    message: "first".to_string(),
                },
            )
            .unwrap();
        storage
            .append_jsonl(
                &file,
                &Record {
                    id: 2,
                    message: "second".to_string(),
                },
            )
            .unwrap();
        storage
            .append_jsonl(
                &file,
                &Record {
                    id: 3,
                    message: "third".to_string(),
                },
            )
            .unwrap();

        // Read them back
        let records: Vec<Record> = storage.read_jsonl(&file).unwrap();

        assert_eq!(records.len(), 3);
        assert_eq!(records[0].id, 1);
        assert_eq!(records[1].id, 2);
        assert_eq!(records[2].id, 3);
    }

    #[test]
    fn test_actor_persistence() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();
        fs::create_dir(repo_root.join(".git")).unwrap();

        let storage = Storage::for_repo(repo_root);

        // Initially no actor
        assert!(storage.read_actor().is_none());

        // Write actor
        storage.write_actor("agent1").unwrap();

        // Read back
        assert_eq!(storage.read_actor(), Some("agent1".to_string()));
    }

    #[test]
    fn test_workspaces_registry() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();
        fs::create_dir(repo_root.join(".git")).unwrap();

        let storage = Storage::for_repo(repo_root.clone());
        storage.init_all().unwrap();

        let mut registry = storage.read_workspaces().unwrap();
        assert!(registry.workspaces.is_empty());

        let workspace_path = repo_root.join(".sv/worktrees/ws1");
        fs::create_dir_all(&workspace_path).unwrap();

        // Add a workspace
        registry
            .insert(WorkspaceEntry::new(
                "ws1".to_string(),
                workspace_path,
                "sv/ws/ws1".to_string(),
                "main".to_string(),
                Some("agent1".to_string()),
                "2024-01-01T00:00:00Z".to_string(),
                None,
            ))
            .unwrap();

        storage.write_workspaces(&registry).unwrap();

        // Read back
        let registry2 = storage.read_workspaces().unwrap();
        assert_eq!(registry2.workspaces.len(), 1);
        assert_eq!(registry2.find("ws1").unwrap().branch, "sv/ws/ws1");
    }

    #[test]
    fn test_ensure_gitignore() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();

        // No existing .gitignore
        ensure_gitignore(&repo_root).unwrap();

        let content = fs::read_to_string(repo_root.join(".gitignore")).unwrap();
        assert!(content.contains("/.sv/"));

        // Running again should not duplicate
        ensure_gitignore(&repo_root).unwrap();

        let content2 = fs::read_to_string(repo_root.join(".gitignore")).unwrap();
        assert_eq!(
            content.matches("/.sv/").count(),
            content2.matches("/.sv/").count()
        );
    }
}
