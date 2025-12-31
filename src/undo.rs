//! Undo operations based on the operation log.
//!
//! Basic semantics:
//! - Ref updates are restored to previous tips
//! - Created paths are removed (unless keep_worktree)
//! - Deleted paths are not restored (error)
//! - Lease changes are reverted when possible

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use git2::{Oid, Repository};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::lease::{Lease, LeaseStatus};
use crate::lock::{self, FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::oplog::{LeaseChange, OpLog, OpRecord, UndoData};
use crate::storage::Storage;

/// Options for undoing an operation.
#[derive(Debug, Clone, Default)]
pub struct UndoOptions {
    pub op_id: Option<Uuid>,
    pub keep_worktree: bool,
}

/// Summary of an undo operation.
#[derive(Debug, Clone, Default)]
pub struct UndoSummary {
    pub op_id: Uuid,
    pub restored_refs: Vec<String>,
    pub removed_paths: Vec<PathBuf>,
    pub skipped_paths: Vec<PathBuf>,
    pub reverted_leases: Vec<String>,
}

/// Undo the last operation (or a specific op_id if provided).
pub fn undo(storage: &Storage, options: UndoOptions) -> Result<UndoSummary> {
    let repo_root = repo_root_from_storage(storage)?;
    let repo = Repository::discover(&repo_root)?;

    let log = OpLog::for_storage(storage);
    let record = select_record(&log, options.op_id)?;
    let undo = record
        .undo_data
        .clone()
        .ok_or_else(|| Error::OperationFailed("operation has no undo data".to_string()))?;

    if !undo.deleted_paths.is_empty() {
        return Err(Error::OperationFailed(format!(
            "cannot restore deleted paths: {}",
            undo.deleted_paths.join(", ")
        )));
    }

    let mut summary = UndoSummary {
        op_id: record.op_id,
        ..UndoSummary::default()
    };

    apply_ref_updates(&repo, &undo, &mut summary)?;
    apply_created_paths(&undo, options.keep_worktree, &mut summary)?;
    apply_lease_changes(storage, &undo.lease_changes, &mut summary)?;

    Ok(summary)
}

fn select_record(log: &OpLog, op_id: Option<Uuid>) -> Result<OpRecord> {
    let mut records = log.read_all()?;
    records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    if let Some(id) = op_id {
        return records
            .into_iter()
            .find(|record| record.op_id == id)
            .ok_or_else(|| Error::OperationFailed(format!("operation not found: {id}")));
    }

    records
        .into_iter()
        .find(|record| record.undo_data.is_some())
        .ok_or_else(|| Error::OperationFailed("no undoable operations found".to_string()))
}

fn apply_ref_updates(repo: &Repository, undo: &UndoData, summary: &mut UndoSummary) -> Result<()> {
    for update in &undo.ref_updates {
        match update.old.as_deref() {
            Some(old) => {
                let oid = Oid::from_str(old)
                    .map_err(|_| Error::OperationFailed(format!("invalid oid: {old}")))?;
                repo.reference(&update.name, oid, true, "sv undo")?;
                summary.restored_refs.push(update.name.clone());
            }
            None => {
                if let Ok(mut reference) = repo.find_reference(&update.name) {
                    reference.delete()?;
                    summary.restored_refs.push(update.name.clone());
                }
            }
        }
    }

    Ok(())
}

fn apply_created_paths(
    undo: &UndoData,
    keep_worktree: bool,
    summary: &mut UndoSummary,
) -> Result<()> {
    for path_str in &undo.created_paths {
        let path = PathBuf::from(path_str);
        if !path.exists() {
            continue;
        }

        if keep_worktree && path.is_dir() {
            summary.skipped_paths.push(path);
            continue;
        }

        remove_path(&path)?;
        summary.removed_paths.push(path);
    }

    Ok(())
}

fn apply_lease_changes(
    storage: &Storage,
    changes: &[LeaseChange],
    summary: &mut UndoSummary,
) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }

    let leases_path = storage.leases_file();
    let lock_path = PathBuf::from(format!("{}.lock", leases_path.display()));
    let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

    let mut leases: Vec<Lease> = storage.read_jsonl(&leases_path)?;
    let mut touched = false;

    for change in changes {
        let id = Uuid::parse_str(&change.lease_id).map_err(|_| {
            Error::OperationFailed(format!("invalid lease id: {}", change.lease_id))
        })?;

        let lease = leases.iter_mut().find(|lease| lease.id == id);
        let lease = match lease {
            Some(lease) => lease,
            None => continue,
        };

        match change.action.as_str() {
            "create" | "add" => {
                lease.status = LeaseStatus::Released;
                lease.status_changed_at = Some(Utc::now());
                lease.status_reason = Some("undo".to_string());
            }
            "release" | "break" | "expire" => {
                lease.status = LeaseStatus::Active;
                lease.status_changed_at = Some(Utc::now());
                lease.status_reason = Some("undo".to_string());
            }
            other => {
                return Err(Error::OperationFailed(format!(
                    "unsupported lease undo action: {other}"
                )));
            }
        }

        summary.reverted_leases.push(change.lease_id.clone());
        touched = true;
    }

    if touched {
        let mut contents = String::new();
        for lease in leases {
            let line = serde_json::to_string(&lease)?;
            contents.push_str(&line);
            contents.push('\n');
        }
        lock::write_atomic(&leases_path, contents.as_bytes())?;
    }

    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn repo_root_from_storage(storage: &Storage) -> Result<PathBuf> {
    let local_dir = storage.local_dir();
    let repo_root = local_dir
        .parent()
        .ok_or_else(|| Error::OperationFailed("failed to resolve repo root".to_string()))?;
    Ok(repo_root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_storage() -> (TempDir, Storage) {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().to_path_buf();
        Repository::init(&repo_root).unwrap();
        let storage = Storage::for_repo(repo_root);
        (temp, storage)
    }

    fn commit(repo: &Repository, message: &str) -> Oid {
        let mut index = repo.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap_or_else(|_| {
            git2::Signature::now("sv-test", "sv-test@example.com").unwrap()
        });

        let parents = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        match parents {
            Some(parent) => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                .unwrap(),
            None => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .unwrap(),
        }
    }

    #[test]
    fn undo_removes_created_paths() {
        let (_temp, storage) = setup_storage();
        let log = OpLog::for_storage(&storage);
        let created_dir = storage.local_dir().parent().unwrap().join("worktree-a");
        fs::create_dir_all(&created_dir).unwrap();

        let mut record = OpRecord::new("sv ws new worktree-a", Some("tester".to_string()));
        record.undo_data = Some(UndoData {
            created_paths: vec![created_dir.to_string_lossy().to_string()],
            ..UndoData::default()
        });
        log.append(&record).unwrap();

        let summary = undo(&storage, UndoOptions::default()).unwrap();
        assert_eq!(summary.op_id, record.op_id);
        assert!(!created_dir.exists());
    }

    #[test]
    fn undo_restores_ref() {
        let (_temp, storage) = setup_storage();
        let repo_root = repo_root_from_storage(&storage).unwrap();
        let repo = Repository::open(repo_root).unwrap();
        let commit_a = commit(&repo, "A");
        let commit_b = commit(&repo, "B");

        repo.reference("refs/heads/feature", commit_b, true, "test")
            .unwrap();

        let log = OpLog::for_storage(&storage);
        let mut record = OpRecord::new("sv onto", Some("tester".to_string()));
        record.undo_data = Some(UndoData {
            ref_updates: vec![crate::oplog::RefUpdate {
                name: "refs/heads/feature".to_string(),
                old: Some(commit_a.to_string()),
                new: Some(commit_b.to_string()),
            }],
            ..UndoData::default()
        });
        log.append(&record).unwrap();

        undo(&storage, UndoOptions::default()).unwrap();
        let updated = repo.find_reference("refs/heads/feature").unwrap();
        assert_eq!(updated.target().unwrap(), commit_a);
    }
}
