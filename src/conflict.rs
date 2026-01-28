//! Conflict tracking for jj-style conflict propagation.
//!
//! This module provides infrastructure for tracking commits that contain
//! unresolved conflicts. Instead of aborting on conflicts, sv can commit
//! the conflicting state with conflict markers and track it for later resolution.

use chrono::{DateTime, Utc};
use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

// =============================================================================
// Conflict Markers
// =============================================================================

/// Standard git conflict marker for "ours" side
pub const CONFLICT_MARKER_OURS: &str = "<<<<<<<";

/// Standard git conflict marker separator
pub const CONFLICT_MARKER_SEP: &str = "=======";

/// Standard git conflict marker for "theirs" side  
pub const CONFLICT_MARKER_THEIRS: &str = ">>>>>>>";

/// Check if content contains git conflict markers.
///
/// Returns true if the content contains the standard conflict marker pattern.
pub fn has_conflict_markers(content: &str) -> bool {
    content.contains(CONFLICT_MARKER_OURS)
        && content.contains(CONFLICT_MARKER_SEP)
        && content.contains(CONFLICT_MARKER_THEIRS)
}

/// Check if a single line is a conflict marker line.
pub fn is_conflict_marker_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with(CONFLICT_MARKER_OURS)
        || trimmed.starts_with(CONFLICT_MARKER_SEP)
        || trimmed.starts_with(CONFLICT_MARKER_THEIRS)
}

/// Find all files in a commit's tree that contain conflict markers.
///
/// Walks the tree and checks each blob for conflict markers.
pub fn find_conflict_markers_in_commit(repo: &Repository, commit_id: Oid) -> Result<Vec<String>> {
    let commit = repo.find_commit(commit_id)?;
    let tree = commit.tree()?;
    find_conflict_markers_in_tree(repo, &tree, "")
}

/// Find all files in a tree that contain conflict markers.
fn find_conflict_markers_in_tree(
    repo: &Repository,
    tree: &git2::Tree,
    prefix: &str,
) -> Result<Vec<String>> {
    let mut conflict_files = Vec::new();

    for entry in tree.iter() {
        let name = entry.name().unwrap_or("");
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", prefix, name)
        };

        match entry.kind() {
            Some(git2::ObjectType::Blob) => {
                let blob = repo.find_blob(entry.id())?;
                if let Ok(content) = std::str::from_utf8(blob.content()) {
                    if has_conflict_markers(content) {
                        conflict_files.push(path);
                    }
                }
            }
            Some(git2::ObjectType::Tree) => {
                let subtree = repo.find_tree(entry.id())?;
                let sub_conflicts = find_conflict_markers_in_tree(repo, &subtree, &path)?;
                conflict_files.extend(sub_conflicts);
            }
            _ => {}
        }
    }

    Ok(conflict_files)
}

/// Check if a working tree file contains conflict markers.
pub fn file_has_conflict_markers(path: &std::path::Path) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    Ok(has_conflict_markers(&content))
}

// =============================================================================
// Conflict Record
// =============================================================================

/// A record of a commit that contains unresolved conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    /// The commit ID that contains conflicts
    pub commit_id: String,

    /// Files within the commit that have conflict markers
    pub files: Vec<String>,

    /// The hoist operation that created this conflict (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist_id: Option<String>,

    /// The original commit being cherry-picked/rebased (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit_id: Option<String>,

    /// When the conflict was first detected
    pub detected_at: DateTime<Utc>,

    /// When the conflict was resolved (None if still unresolved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,

    /// Optional note about the conflict
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ConflictRecord {
    /// Create a new conflict record.
    pub fn new(commit_id: impl Into<String>, files: Vec<String>) -> Self {
        Self {
            commit_id: commit_id.into(),
            files,
            hoist_id: None,
            source_commit_id: None,
            detected_at: Utc::now(),
            resolved_at: None,
            note: None,
        }
    }

    /// Set the hoist ID that created this conflict.
    pub fn with_hoist_id(mut self, hoist_id: impl Into<String>) -> Self {
        self.hoist_id = Some(hoist_id.into());
        self
    }

    /// Set the source commit ID.
    pub fn with_source_commit(mut self, source_id: impl Into<String>) -> Self {
        self.source_commit_id = Some(source_id.into());
        self
    }

    /// Set a note about the conflict.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Check if this conflict is resolved.
    pub fn is_resolved(&self) -> bool {
        self.resolved_at.is_some()
    }

    /// Mark this conflict as resolved.
    pub fn mark_resolved(&mut self) {
        self.resolved_at = Some(Utc::now());
    }

    /// Mark this conflict as resolved at a specific time.
    pub fn mark_resolved_at(&mut self, at: DateTime<Utc>) {
        self.resolved_at = Some(at);
    }
}

// =============================================================================
// Conflict Store
// =============================================================================

/// In-memory store for conflict records.
#[derive(Debug, Default)]
pub struct ConflictStore {
    records: Vec<ConflictRecord>,
}

impl ConflictStore {
    /// Create a new empty conflict store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a store from a vector of records.
    pub fn from_vec(records: Vec<ConflictRecord>) -> Self {
        Self { records }
    }

    /// Add a conflict record.
    pub fn add(&mut self, record: ConflictRecord) {
        self.records.push(record);
    }

    /// Get all records.
    pub fn all(&self) -> &[ConflictRecord] {
        &self.records
    }

    /// Get all unresolved conflicts.
    pub fn unresolved(&self) -> impl Iterator<Item = &ConflictRecord> {
        self.records.iter().filter(|r| !r.is_resolved())
    }

    /// Get all resolved conflicts.
    pub fn resolved(&self) -> impl Iterator<Item = &ConflictRecord> {
        self.records.iter().filter(|r| r.is_resolved())
    }

    /// Find a conflict record by commit ID.
    pub fn find_by_commit(&self, commit_id: &str) -> Option<&ConflictRecord> {
        self.records.iter().find(|r| r.commit_id == commit_id)
    }

    /// Find a conflict record by commit ID (mutable).
    pub fn find_by_commit_mut(&mut self, commit_id: &str) -> Option<&mut ConflictRecord> {
        self.records.iter_mut().find(|r| r.commit_id == commit_id)
    }

    /// Find conflicts from a specific hoist operation.
    pub fn find_by_hoist(&self, hoist_id: &str) -> impl Iterator<Item = &ConflictRecord> {
        let hoist_id = hoist_id.to_string();
        self.records
            .iter()
            .filter(move |r| r.hoist_id.as_ref() == Some(&hoist_id))
    }

    /// Mark a conflict as resolved by commit ID.
    pub fn mark_resolved(&mut self, commit_id: &str) -> bool {
        if let Some(record) = self.find_by_commit_mut(commit_id) {
            record.mark_resolved();
            true
        } else {
            false
        }
    }

    /// Check if a commit has an unresolved conflict.
    pub fn has_unresolved_conflict(&self, commit_id: &str) -> bool {
        self.records
            .iter()
            .any(|r| r.commit_id == commit_id && !r.is_resolved())
    }

    /// Count unresolved conflicts.
    pub fn unresolved_count(&self) -> usize {
        self.records.iter().filter(|r| !r.is_resolved()).count()
    }

    /// Get the underlying records (consuming).
    pub fn into_vec(self) -> Vec<ConflictRecord> {
        self.records
    }
}

// =============================================================================
// Conflict Index Utilities
// =============================================================================

/// Write a conflicting index to a tree, including conflict markers in files.
///
/// This is used to create a commit that contains the conflict state.
/// The resulting tree will have files with conflict markers embedded.
pub fn write_conflict_tree(repo: &Repository, index: &mut git2::Index) -> Result<Oid> {
    // Get all conflicting entries
    let conflicts: Vec<_> = index
        .conflicts()?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if conflicts.is_empty() {
        // No conflicts, just write the index as-is
        return Ok(index.write_tree_to(repo)?);
    }

    // For each conflict, create a blob with conflict markers
    for conflict in &conflicts {
        let path = conflict
            .our
            .as_ref()
            .or(conflict.their.as_ref())
            .or(conflict.ancestor.as_ref())
            .map(|e| String::from_utf8_lossy(&e.path).to_string())
            .ok_or_else(|| Error::OperationFailed("Conflict without path".to_string()))?;

        let merged_content = create_conflict_content(repo, conflict)?;
        let blob_id = repo.blob(merged_content.as_bytes())?;

        // Remove conflict entries and add the merged blob
        index.remove_path(std::path::Path::new(&path))?;

        // Determine file mode from available entries
        let mode = conflict
            .our
            .as_ref()
            .or(conflict.their.as_ref())
            .map(|e| e.mode)
            .unwrap_or(0o100644);

        index.add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode,
            uid: 0,
            gid: 0,
            file_size: merged_content.len() as u32,
            id: blob_id,
            flags: 0,
            flags_extended: 0,
            path: path.into_bytes(),
        })?;
    }

    Ok(index.write_tree_to(repo)?)
}

/// Create file content with conflict markers from a conflict entry.
fn create_conflict_content(repo: &Repository, conflict: &git2::IndexConflict) -> Result<String> {
    let ours_content = if let Some(ref entry) = conflict.our {
        let blob = repo.find_blob(entry.id)?;
        String::from_utf8_lossy(blob.content()).to_string()
    } else {
        String::new()
    };

    let theirs_content = if let Some(ref entry) = conflict.their {
        let blob = repo.find_blob(entry.id)?;
        String::from_utf8_lossy(blob.content()).to_string()
    } else {
        String::new()
    };

    let ours_label = conflict
        .our
        .as_ref()
        .map(|e| String::from_utf8_lossy(&e.path).to_string())
        .unwrap_or_else(|| "ours".to_string());

    let theirs_label = conflict
        .their
        .as_ref()
        .map(|e| String::from_utf8_lossy(&e.path).to_string())
        .unwrap_or_else(|| "theirs".to_string());

    // Simple conflict format - in practice, you'd want a proper 3-way merge
    // with line-level conflict markers, but this gives the basic structure
    Ok(format!(
        "{} {}\n{}{}\n{}{} {}\n",
        CONFLICT_MARKER_OURS,
        ours_label,
        ours_content,
        CONFLICT_MARKER_SEP,
        theirs_content,
        CONFLICT_MARKER_THEIRS,
        theirs_label
    ))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_conflict_markers() {
        let no_conflict = "fn main() {\n    println!(\"hello\");\n}\n";
        assert!(!has_conflict_markers(no_conflict));

        let with_conflict = r#"fn main() {
<<<<<<< HEAD
    println!("hello");
=======
    println!("world");
>>>>>>> feature
}
"#;
        assert!(has_conflict_markers(with_conflict));

        // Partial markers don't count
        let partial = "<<<<<<< HEAD\nsome content\n";
        assert!(!has_conflict_markers(partial));
    }

    #[test]
    fn test_is_conflict_marker_line() {
        assert!(is_conflict_marker_line("<<<<<<< HEAD"));
        assert!(is_conflict_marker_line("======="));
        assert!(is_conflict_marker_line(">>>>>>> feature"));
        assert!(is_conflict_marker_line("  <<<<<<< indented"));
        assert!(!is_conflict_marker_line("normal code"));
        assert!(!is_conflict_marker_line("// <<<<<<< in comment"));
    }

    #[test]
    fn test_conflict_record_creation() {
        let record = ConflictRecord::new("abc123", vec!["src/main.rs".to_string()])
            .with_hoist_id("hoist-1")
            .with_source_commit("def456")
            .with_note("Conflicting changes to main function");

        assert_eq!(record.commit_id, "abc123");
        assert_eq!(record.files, vec!["src/main.rs"]);
        assert_eq!(record.hoist_id, Some("hoist-1".to_string()));
        assert_eq!(record.source_commit_id, Some("def456".to_string()));
        assert!(!record.is_resolved());
    }

    #[test]
    fn test_conflict_record_resolution() {
        let mut record = ConflictRecord::new("abc123", vec!["src/main.rs".to_string()]);
        assert!(!record.is_resolved());

        record.mark_resolved();
        assert!(record.is_resolved());
        assert!(record.resolved_at.is_some());
    }

    #[test]
    fn test_conflict_store() {
        let mut store = ConflictStore::new();

        store.add(ConflictRecord::new("commit1", vec!["file1.rs".to_string()]));
        store.add(ConflictRecord::new("commit2", vec!["file2.rs".to_string()]));

        assert_eq!(store.all().len(), 2);
        assert_eq!(store.unresolved_count(), 2);

        store.mark_resolved("commit1");
        assert_eq!(store.unresolved_count(), 1);
        assert!(store.find_by_commit("commit1").unwrap().is_resolved());
        assert!(!store.find_by_commit("commit2").unwrap().is_resolved());
    }

    #[test]
    fn test_conflict_store_find_by_hoist() {
        let mut store = ConflictStore::new();

        store.add(
            ConflictRecord::new("commit1", vec!["file1.rs".to_string()]).with_hoist_id("hoist-1"),
        );
        store.add(
            ConflictRecord::new("commit2", vec!["file2.rs".to_string()]).with_hoist_id("hoist-1"),
        );
        store.add(
            ConflictRecord::new("commit3", vec!["file3.rs".to_string()]).with_hoist_id("hoist-2"),
        );

        let hoist1_conflicts: Vec<_> = store.find_by_hoist("hoist-1").collect();
        assert_eq!(hoist1_conflicts.len(), 2);

        let hoist2_conflicts: Vec<_> = store.find_by_hoist("hoist-2").collect();
        assert_eq!(hoist2_conflicts.len(), 1);
    }
}
