//! Operation log storage for sv
//!
//! Stores append-only operation records under `.git/sv/oplog/`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::lock::{self, FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::storage::Storage;

/// Operation log record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpRecord {
    pub op_id: Uuid,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_workspaces: Vec<String>,
    pub outcome: OpOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<OpDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo_data: Option<UndoData>,
}

impl OpRecord {
    pub fn new(command: impl Into<String>, actor: Option<String>) -> Self {
        Self {
            op_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            actor,
            command: command.into(),
            affected_refs: Vec::new(),
            affected_workspaces: Vec::new(),
            outcome: OpOutcome::success(),
            details: None,
            undo_data: None,
        }
    }
}

/// Operation outcome summary
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpOutcome {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl OpOutcome {
    pub fn success() -> Self {
        Self {
            status: "success".to_string(),
            message: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: "failed".to_string(),
            message: Some(message.into()),
        }
    }
}

/// Undo metadata for an operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UndoData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ref_updates: Vec<RefUpdate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lease_changes: Vec<LeaseChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_changes: Vec<WorkspaceChange>,
}

/// Optional operation-specific details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OpDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitDetails>,
}

/// Commit details for op log entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitDetails {
    pub commit_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_protected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_lease: Option<bool>,
}

/// Ref update for undo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RefUpdate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

/// Lease change record for undo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeaseChange {
    pub lease_id: String,
    pub action: String,
}

/// Workspace change record for undo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceChange {
    pub name: String,
    pub action: String, // "create", "remove", "register"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// Operation log manager
#[derive(Debug, Clone)]
pub struct OpLog {
    dir: PathBuf,
}

impl OpLog {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn for_storage(storage: &Storage) -> Self {
        Self::new(storage.oplog_dir())
    }

    /// Append a new operation record to the log
    pub fn append(&self, record: &OpRecord) -> Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        let lock_path = oplog_lock_path(&self.dir);
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

        let file_name = record_filename(record);
        let path = self.dir.join(file_name);
        if path.exists() {
            return Err(Error::OperationFailed(format!(
                "oplog entry already exists: {}",
                path.display()
            )));
        }

        let json = serde_json::to_vec_pretty(record)?;
        lock::write_atomic(&path, &json)?;
        Ok(path)
    }

    /// Read all operation records (sorted by filename)
    pub fn read_all(&self) -> Result<Vec<OpRecord>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        let lock_path = oplog_lock_path(&self.dir);
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;

        let mut paths: Vec<PathBuf> = fs::read_dir(&self.dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();

        paths.sort();

        let mut records = Vec::new();
        for path in paths {
            let content = fs::read_to_string(&path)?;
            let record: OpRecord = serde_json::from_str(&content)?;
            records.push(record);
        }

        Ok(records)
    }

    /// Read operation records filtered and sorted by timestamp desc
    pub fn read_filtered(&self, filter: &OpLogFilter, limit: Option<usize>) -> Result<Vec<OpRecord>> {
        let mut records = self.read_all()?;
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let mut filtered: Vec<OpRecord> = records
            .into_iter()
            .filter(|record| filter.matches(record))
            .collect();

        if let Some(limit) = limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }
}

/// Filter for selecting operation log entries
#[derive(Debug, Clone, Default)]
pub struct OpLogFilter {
    pub actor: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub operation: Option<String>,
}

impl OpLogFilter {
    pub fn matches(&self, record: &OpRecord) -> bool {
        if let Some(actor) = &self.actor {
            if record.actor.as_deref() != Some(actor.as_str()) {
                return false;
            }
        }

        if let Some(since) = &self.since {
            if &record.timestamp < since {
                return false;
            }
        }

        if let Some(until) = &self.until {
            if &record.timestamp > until {
                return false;
            }
        }

        if let Some(operation) = &self.operation {
            let record_op = operation_from_command(&record.command);
            if record_op != operation {
                return false;
            }
        }

        true
    }
}

/// Format a single operation record for human-readable output
pub fn format_record(record: &OpRecord) -> String {
    let ts = record.timestamp.to_rfc3339();
    let actor = record.actor.as_deref().unwrap_or("-");
    let refs = if record.affected_refs.is_empty() {
        "-".to_string()
    } else {
        record.affected_refs.join(",")
    };
    let workspaces = if record.affected_workspaces.is_empty() {
        "-".to_string()
    } else {
        record.affected_workspaces.join(",")
    };
    let outcome = match &record.outcome.message {
        Some(msg) => format!("{} ({})", record.outcome.status, msg),
        None => record.outcome.status.clone(),
    };

    let details = format_details(record);

    format!(
        "{ts} {op_id} actor={actor} outcome={outcome} command=\"{command}\" refs=[{refs}] workspaces=[{workspaces}]",
        op_id = record.op_id,
        command = record.command
    )
    + &details
}

/// Format multiple records as lines
pub fn format_records(records: &[OpRecord]) -> String {
    records.iter().map(format_record).collect::<Vec<_>>().join("\n")
}

fn format_details(record: &OpRecord) -> String {
    let Some(details) = &record.details else {
        return String::new();
    };

    if let Some(commit) = &details.commit {
        let mut parts = Vec::new();
        parts.push(format!("commit={}", commit.commit_hash));
        if let Some(change_id) = &commit.change_id {
            parts.push(format!("change_id={change_id}"));
        }
        if !commit.files.is_empty() {
            parts.push(format!("files={}", commit.files.len()));
        }
        let mut overrides = Vec::new();
        if commit.allow_protected.unwrap_or(false) {
            overrides.push("allow_protected");
        }
        if commit.force_lease.unwrap_or(false) {
            overrides.push("force_lease");
        }
        if !overrides.is_empty() {
            parts.push(format!("overrides={}", overrides.join(",")));
        }
        return format!(" details=[{}]", parts.join(" "));
    }

    String::new()
}

fn operation_from_command(command: &str) -> &str {
    let mut parts = command.split_whitespace();
    let first = parts.next().unwrap_or_default();
    if first == "sv" {
        parts.next().unwrap_or(first)
    } else {
        first
    }
}

fn oplog_lock_path(dir: &Path) -> PathBuf {
    dir.join("oplog.lock")
}

fn record_filename(record: &OpRecord) -> String {
    let ts = record.timestamp.format("%Y%m%dT%H%M%S%.3fZ");
    format!("{}-{}.json", ts, record.op_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn append_and_read_records() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("oplog");
        let log = OpLog::new(dir);

        let record = OpRecord::new("sv init", Some("agent1".to_string()));
        let path = log.append(&record).unwrap();
        assert!(path.exists());

        let records = log.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].op_id, record.op_id);
        assert_eq!(records[0].command, "sv init");
    }

    #[test]
    fn op_record_defaults() {
        let record = OpRecord::new("sv status", None);
        assert_eq!(record.outcome.status, "success");
        assert!(record.affected_refs.is_empty());
        assert!(record.affected_workspaces.is_empty());
        assert!(record.undo_data.is_none());
    }
}
