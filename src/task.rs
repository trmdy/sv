//! Task management for sv.
//!
//! Tasks are stored as append-only events in `.tasks/tasks.jsonl` (tracked)
//! and `.git/sv/tasks.jsonl` (shared across worktrees in a clone).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::config::TasksConfig;
use crate::error::{Error, Result};
use crate::lock::{FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::storage::Storage;

const TASKS_DIR: &str = ".tasks";
const TASKS_LOG: &str = "tasks.jsonl";
const TASKS_SNAPSHOT: &str = "tasks.snapshot.json";
const TASKS_SCHEMA_VERSION: &str = "sv.tasks.v1";
const TASK_ID_MIN_LEN: usize = 3;
const TASK_ID_DELIMS: [&str; 2] = ["-", "/"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskEventType {
    TaskCreated,
    TaskStarted,
    TaskStatusChanged,
    TaskClosed,
    TaskCommented,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub event_id: String,
    pub task_id: String,
    #[serde(rename = "type")]
    pub event_type: TaskEventType,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl TaskEvent {
    pub fn new(event_type: TaskEventType, task_id: impl Into<String>) -> Self {
        Self {
            event_id: Ulid::new().to_string(),
            task_id: task_id.into(),
            event_type,
            timestamp: Utc::now(),
            actor: None,
            title: None,
            body: None,
            status: None,
            workspace_id: None,
            workspace: None,
            branch: None,
            comment: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    pub comments_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_comment_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub tasks: Vec<TaskRecord>,
}

impl TaskSnapshot {
    pub fn empty() -> Self {
        Self {
            schema_version: TASKS_SCHEMA_VERSION.to_string(),
            generated_at: Utc::now(),
            tasks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskComment {
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub comment: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDetails {
    pub task: TaskRecord,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<TaskComment>,
    pub events: usize,
}

#[derive(Debug, Clone)]
pub struct TaskStore {
    storage: Storage,
    config: TasksConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSyncReport {
    pub total_events: usize,
    pub total_tasks: usize,
    pub compacted: bool,
    pub removed_events: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskCompactReport {
    pub before_events: usize,
    pub after_events: usize,
    pub removed_events: usize,
    pub compacted_tasks: usize,
}

#[derive(Debug, Clone)]
pub struct CompactionPolicy {
    pub older_than: Option<chrono::Duration>,
    pub max_log_mb: Option<u64>,
}

impl TaskStore {
    pub fn new(storage: Storage, config: TasksConfig) -> Self {
        Self { storage, config }
    }

    pub fn config(&self) -> &TasksConfig {
        &self.config
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.storage.workspace_root().join(TASKS_DIR)
    }

    pub fn tracked_log_path(&self) -> PathBuf {
        self.tasks_dir().join(TASKS_LOG)
    }

    pub fn tracked_snapshot_path(&self) -> PathBuf {
        self.tasks_dir().join(TASKS_SNAPSHOT)
    }

    pub fn shared_log_path(&self) -> PathBuf {
        self.storage.shared_dir().join(TASKS_LOG)
    }

    pub fn shared_snapshot_path(&self) -> PathBuf {
        self.storage.shared_dir().join(TASKS_SNAPSHOT)
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(self.tasks_dir())?;
        std::fs::create_dir_all(self.storage.shared_dir())?;
        Ok(())
    }

    pub fn append_event(&self, event: TaskEvent) -> Result<()> {
        self.ensure_dirs()?;
        self.append_event_to_log(&self.tracked_log_path(), &event)?;
        self.append_event_to_log(&self.shared_log_path(), &event)?;
        self.apply_event_to_snapshot(&self.tracked_snapshot_path(), &event)?;
        self.apply_event_to_snapshot(&self.shared_snapshot_path(), &event)?;
        Ok(())
    }

    pub fn list(&self, status: Option<&str>) -> Result<Vec<TaskRecord>> {
        let snapshot = self.load_snapshot_prefer_shared()?;
        let mut tasks = snapshot.tasks;
        if let Some(status) = status {
            let status = status.trim();
            self.validate_status(status)?;
            tasks.retain(|task| task.status == status);
        }
        Ok(tasks)
    }

    pub fn generate_task_id(&self) -> Result<String> {
        let prefix = self.config.id_prefix.trim();
        let prefix_norm = normalize_id(prefix);
        let snapshot = self.snapshot_readonly()?;
        let mut existing_suffixes = HashSet::new();
        for task in snapshot.tasks {
            let id_norm = normalize_id(&task.id);
            let suffix = extract_suffix_normalized(&id_norm, &prefix_norm);
            existing_suffixes.insert(suffix.to_string());
        }

        loop {
            let base = Ulid::new().to_string().to_lowercase();
            let max_len = base.len();
            for len in TASK_ID_MIN_LEN..=max_len {
                let candidate = &base[..len];
                if !existing_suffixes.contains(candidate) {
                    return Ok(format!("{}-{}", prefix, candidate));
                }
            }
        }
    }

    pub fn resolve_task_id(&self, input: &str) -> Result<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument("task id cannot be empty".to_string()));
        }

        let prefix = self.config.id_prefix.trim();
        let prefix_norm = normalize_id(prefix);
        let trimmed_norm = normalize_id(trimmed);
        let candidate_norm = strip_prefix_normalized(&trimmed_norm, &prefix_norm)
            .unwrap_or(trimmed_norm.as_str())
            .to_string();
        if candidate_norm.is_empty() {
            return Err(Error::InvalidArgument("task id cannot be empty".to_string()));
        }

        let snapshot = self.snapshot_readonly()?;
        let mut exact: Vec<String> = Vec::new();
        let mut matches: Vec<String> = Vec::new();

        for task in snapshot.tasks {
            let id_norm = normalize_id(&task.id);
            let suffix_norm = extract_suffix_normalized(&id_norm, &prefix_norm);
            if id_norm == trimmed_norm || suffix_norm == trimmed_norm {
                exact.push(task.id.clone());
                continue;
            }
            if suffix_norm.starts_with(&candidate_norm) {
                matches.push(task.id.clone());
            }
        }

        if exact.len() == 1 {
            return Ok(exact.remove(0));
        }
        if exact.len() > 1 {
            return Err(Error::InvalidArgument(format!(
                "ambiguous task id '{}': {}",
                trimmed,
                exact.join(", ")
            )));
        }

        matches.sort();
        matches.dedup();
        if matches.is_empty() {
            return Err(Error::InvalidArgument(format!(
                "task not found: {trimmed}"
            )));
        }
        if matches.len() > 1 {
            return Err(Error::InvalidArgument(format!(
                "ambiguous task id '{}': {}",
                trimmed,
                matches.join(", ")
            )));
        }
        Ok(matches[0].clone())
    }

    pub fn snapshot_readonly(&self) -> Result<TaskSnapshot> {
        if let Some(snapshot) = self.load_snapshot(&self.shared_snapshot_path())? {
            return Ok(snapshot);
        }
        if self.shared_log_path().exists() {
            let events = self.load_events(&self.shared_log_path())?;
            return self.build_snapshot(&events);
        }
        if let Some(snapshot) = self.load_snapshot(&self.tracked_snapshot_path())? {
            return Ok(snapshot);
        }
        let events = self.load_events(&self.tracked_log_path())?;
        self.build_snapshot(&events)
    }

    pub fn auto_compaction_policy(&self) -> Result<Option<CompactionPolicy>> {
        if !self.config.compaction.auto {
            return Ok(None);
        }
        let duration = crate::lease::parse_duration(&self.config.compaction.older_than)?;
        Ok(Some(CompactionPolicy {
            older_than: Some(duration),
            max_log_mb: Some(self.config.compaction.max_log_mb),
        }))
    }

    pub fn details(&self, task_id: &str) -> Result<TaskDetails> {
        let events = self.load_merged_events()?;
        let mut filtered: Vec<TaskEvent> = events
            .into_iter()
            .filter(|event| event.task_id == task_id)
            .collect();
        if filtered.is_empty() {
            return Err(Error::InvalidArgument(format!(
                "task not found: {task_id}"
            )));
        }
        sort_events(&mut filtered);
        let snapshot = self.build_snapshot(&filtered)?;
        let task = snapshot
            .tasks
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| Error::InvalidArgument(format!("task not found: {task_id}")))?;
        let comments = filtered
            .iter()
            .filter_map(|event| {
                if event.event_type == TaskEventType::TaskCommented {
                    event.comment.as_ref().map(|comment| TaskComment {
                        timestamp: event.timestamp,
                        actor: event.actor.clone(),
                        comment: comment.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
        Ok(TaskDetails {
            task,
            comments,
            events: filtered.len(),
        })
    }

    pub fn sync(&self, policy: Option<CompactionPolicy>) -> Result<TaskSyncReport> {
        self.ensure_dirs()?;
        let tracked = self.load_events(&self.tracked_log_path())?;
        let shared = self.load_events(&self.shared_log_path())?;
        let mut merged = merge_events(tracked, shared);
        sort_events(&mut merged);

        let mut compacted = false;
        let mut removed_events = 0;

        if let Some(policy) = policy {
            if self.should_auto_compact(&merged, &policy)? {
                let (compacted_events, report) = self.compact_events(&merged, policy)?;
                removed_events = report.removed_events;
                compacted = removed_events > 0;
                merged = compacted_events;
            }
        }

        let snapshot = self.build_snapshot(&merged)?;
        self.write_events(&self.tracked_log_path(), &merged)?;
        self.write_events(&self.shared_log_path(), &merged)?;
        self.write_snapshot(&self.tracked_snapshot_path(), &snapshot)?;
        self.write_snapshot(&self.shared_snapshot_path(), &snapshot)?;

        Ok(TaskSyncReport {
            total_events: merged.len(),
            total_tasks: snapshot.tasks.len(),
            compacted,
            removed_events,
        })
    }

    pub fn compact(
        &self,
        policy: CompactionPolicy,
    ) -> Result<(Vec<TaskEvent>, TaskCompactReport)> {
        let events = self.load_merged_events()?;
        self.compact_events(&events, policy)
    }

    pub fn replace_events(&self, events: &[TaskEvent]) -> Result<()> {
        let snapshot = self.build_snapshot(events)?;
        self.write_events(&self.tracked_log_path(), events)?;
        self.write_events(&self.shared_log_path(), events)?;
        self.write_snapshot(&self.tracked_snapshot_path(), &snapshot)?;
        self.write_snapshot(&self.shared_snapshot_path(), &snapshot)?;
        Ok(())
    }

    pub fn active_tasks_for_workspaces(
        &self,
        workspace_ids: &[String],
        workspace_names: &[String],
    ) -> Result<Vec<TaskRecord>> {
        let snapshot = self.load_snapshot_prefer_shared()?;
        let closed = self.closed_statuses();
        Ok(snapshot
            .tasks
            .into_iter()
            .filter(|task| {
                if closed.contains(&task.status) {
                    return false;
                }
                let id_match = task
                    .workspace_id
                    .as_ref()
                    .map(|id| workspace_ids.contains(id))
                    .unwrap_or(false);
                let name_match = task
                    .workspace
                    .as_ref()
                    .map(|name| workspace_names.contains(name))
                    .unwrap_or(false);
                id_match || name_match
            })
            .collect())
    }

    fn compact_events(
        &self,
        events: &[TaskEvent],
        policy: CompactionPolicy,
    ) -> Result<(Vec<TaskEvent>, TaskCompactReport)> {
        let mut grouped: HashMap<String, Vec<TaskEvent>> = HashMap::new();
        for event in events {
            grouped
                .entry(event.task_id.clone())
                .or_default()
                .push(event.clone());
        }

        let cutoff = policy
            .older_than
            .map(|duration| Utc::now() - duration);
        let closed_statuses = self.closed_statuses();

        let mut keep_ids = HashSet::new();
        let mut compacted_tasks = 0;

        for (_task_id, mut task_events) in grouped {
            sort_events(&mut task_events);
            if task_events.is_empty() {
                continue;
            }

            let last_event_time = task_events
                .last()
                .map(|event| event.timestamp)
                .unwrap_or_else(Utc::now);

            if let Some(cutoff) = cutoff {
                if last_event_time >= cutoff {
                    for event in task_events {
                        keep_ids.insert(event.event_id.clone());
                    }
                    continue;
                }
            }

            let status = final_status(&task_events, &self.config)?;
            if !closed_statuses.contains(&status) {
                for event in task_events {
                    keep_ids.insert(event.event_id.clone());
                }
                continue;
            }

            compacted_tasks += 1;
            if let Some(first_create) =
                task_events.iter().find(|event| event.event_type == TaskEventType::TaskCreated)
            {
                keep_ids.insert(first_create.event_id.clone());
            }

            for event in task_events.iter().filter(|event| {
                event.event_type == TaskEventType::TaskCommented
                    || event.event_type == TaskEventType::TaskStarted
            }) {
                keep_ids.insert(event.event_id.clone());
            }

            if let Some(last_status) = task_events.iter().rev().find(|event| {
                event.event_type == TaskEventType::TaskClosed
                    || (event.event_type == TaskEventType::TaskStatusChanged
                        && event.status.as_deref().map(|s| closed_statuses.contains(s)) == Some(true))
            }) {
                keep_ids.insert(last_status.event_id.clone());
            }
        }

        let mut compacted: Vec<TaskEvent> = events
            .iter()
            .filter(|event| keep_ids.contains(&event.event_id))
            .cloned()
            .collect();
        sort_events(&mut compacted);

        let report = TaskCompactReport {
            before_events: events.len(),
            after_events: compacted.len(),
            removed_events: events.len().saturating_sub(compacted.len()),
            compacted_tasks,
        };

        Ok((compacted, report))
    }

    fn should_auto_compact(
        &self,
        events: &[TaskEvent],
        policy: &CompactionPolicy,
    ) -> Result<bool> {
        let max_log_mb = policy.max_log_mb;
        if max_log_mb.is_none() && policy.older_than.is_none() {
            return Ok(false);
        }

        if let Some(max_log_mb) = max_log_mb {
            let size_mb = self
                .tracked_log_path()
                .metadata()
                .ok()
                .map(|meta| meta.len() / (1024 * 1024))
                .unwrap_or(0);
            if size_mb < max_log_mb {
                return Ok(false);
            }
        }

        if let Some(duration) = policy.older_than {
            let cutoff = Utc::now() - duration;
            let has_old = events.iter().any(|event| event.timestamp < cutoff);
            if !has_old {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn load_snapshot_prefer_shared(&self) -> Result<TaskSnapshot> {
        if let Some(snapshot) = self.load_snapshot(&self.shared_snapshot_path())? {
            return Ok(snapshot);
        }
        if self.shared_log_path().exists() {
            let events = self.load_events(&self.shared_log_path())?;
            let snapshot = self.build_snapshot(&events)?;
            let _ = self.write_snapshot(&self.shared_snapshot_path(), &snapshot);
            return Ok(snapshot);
        }
        if let Some(snapshot) = self.load_snapshot(&self.tracked_snapshot_path())? {
            return Ok(snapshot);
        }
        let events = self.load_events(&self.tracked_log_path())?;
        self.build_snapshot(&events)
    }

    fn build_snapshot(&self, events: &[TaskEvent]) -> Result<TaskSnapshot> {
        let mut map: HashMap<String, TaskRecord> = HashMap::new();
        let mut sorted = events.to_vec();
        sort_events(&mut sorted);
        for event in sorted {
            apply_event(&mut map, &event, &self.config)?;
        }

        let mut tasks: Vec<TaskRecord> = map.into_values().collect();
        tasks.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(TaskSnapshot {
            schema_version: TASKS_SCHEMA_VERSION.to_string(),
            generated_at: Utc::now(),
            tasks,
        })
    }

    fn load_merged_events(&self) -> Result<Vec<TaskEvent>> {
        let tracked = self.load_events(&self.tracked_log_path())?;
        let shared = self.load_events(&self.shared_log_path())?;
        let mut merged = merge_events(tracked, shared);
        sort_events(&mut merged);
        Ok(merged)
    }

    fn load_events(&self, path: &Path) -> Result<Vec<TaskEvent>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        self.storage.read_jsonl(path)
    }

    fn write_events(&self, path: &Path, events: &[TaskEvent]) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        let mut buffer = Vec::new();
        for event in events {
            let json = serde_json::to_string(event)?;
            buffer.extend_from_slice(json.as_bytes());
            buffer.push(b'\n');
        }
        self.storage.write_atomic(path, &buffer)
    }

    fn append_event_to_log(&self, path: &Path, event: &TaskEvent) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        self.storage.append_jsonl(path, event)
    }

    fn load_snapshot(&self, path: &Path) -> Result<Option<TaskSnapshot>> {
        if !path.exists() {
            return Ok(None);
        }
        let snapshot = self.storage.read_json(path)?;
        Ok(Some(snapshot))
    }

    fn write_snapshot(&self, path: &Path, snapshot: &TaskSnapshot) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        self.storage.write_json(path, snapshot)
    }

    fn apply_event_to_snapshot(&self, path: &Path, event: &TaskEvent) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        let mut snapshot = self
            .load_snapshot(path)?
            .unwrap_or_else(TaskSnapshot::empty);
        let mut map: HashMap<String, TaskRecord> = snapshot
            .tasks
            .drain(..)
            .map(|task| (task.id.clone(), task))
            .collect();
        apply_event(&mut map, event, &self.config)?;
        let mut tasks: Vec<TaskRecord> = map.into_values().collect();
        tasks.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        snapshot.tasks = tasks;
        snapshot.generated_at = Utc::now();
        self.storage.write_json(path, &snapshot)
    }

    pub fn validate_status(&self, status: &str) -> Result<()> {
        if self
            .config
            .statuses
            .iter()
            .any(|value| value == status)
        {
            Ok(())
        } else {
            Err(Error::InvalidArgument(format!(
                "unknown task status '{status}'"
            )))
        }
    }

    fn closed_statuses(&self) -> HashSet<String> {
        self.config
            .closed_statuses
            .iter()
            .map(|status| status.to_string())
            .collect()
    }
}

fn merge_events(mut a: Vec<TaskEvent>, b: Vec<TaskEvent>) -> Vec<TaskEvent> {
    let mut seen = HashSet::new();
    a.retain(|event| seen.insert(event.event_id.clone()));
    for event in b {
        if seen.insert(event.event_id.clone()) {
            a.push(event);
        }
    }
    a
}

fn sort_events(events: &mut Vec<TaskEvent>) {
    events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
}

fn final_status(events: &[TaskEvent], config: &TasksConfig) -> Result<String> {
    let mut status = config.default_status.clone();
    for event in events {
        if let Some(next) = event_status(event, config)? {
            status = next;
        }
    }
    Ok(status)
}

fn event_status(event: &TaskEvent, config: &TasksConfig) -> Result<Option<String>> {
    let status = match event.event_type {
        TaskEventType::TaskCreated => event
            .status
            .clone()
            .unwrap_or_else(|| config.default_status.clone()),
        TaskEventType::TaskStarted => event
            .status
            .clone()
            .unwrap_or_else(|| config.in_progress_status.clone()),
        TaskEventType::TaskStatusChanged => {
            event.status.clone().ok_or_else(|| {
                Error::InvalidArgument(format!(
                    "status missing for task event {}",
                    event.event_id
                ))
            })?
        }
        TaskEventType::TaskClosed => event
            .status
            .clone()
            .unwrap_or_else(|| {
                config
                    .closed_statuses
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "closed".to_string())
            }),
        TaskEventType::TaskCommented => return Ok(None),
    };

    if !config.statuses.iter().any(|value| value == &status) {
        return Err(Error::InvalidArgument(format!(
            "unknown task status '{status}'"
        )));
    }

    Ok(Some(status))
}

fn apply_event(
    map: &mut HashMap<String, TaskRecord>,
    event: &TaskEvent,
    config: &TasksConfig,
) -> Result<()> {
    match event.event_type {
        TaskEventType::TaskCreated => {
            if map.contains_key(&event.task_id) {
                return Err(Error::InvalidArgument(format!(
                    "task already exists: {}",
                    event.task_id
                )));
            }

            let title = event.title.clone().ok_or_else(|| {
                Error::InvalidArgument(format!("missing title for {}", event.task_id))
            })?;

            let status = event
                .status
                .clone()
                .unwrap_or_else(|| config.default_status.clone());
            if !config.statuses.iter().any(|value| value == &status) {
                return Err(Error::InvalidArgument(format!(
                    "unknown task status '{status}'"
                )));
            }

            let now = event.timestamp;
            map.insert(
                event.task_id.clone(),
                TaskRecord {
                    id: event.task_id.clone(),
                    title,
                    status,
                    created_at: now,
                    updated_at: now,
                    created_by: event.actor.clone(),
                    updated_by: event.actor.clone(),
                    body: event.body.clone(),
                    workspace_id: event.workspace_id.clone(),
                    workspace: event.workspace.clone(),
                    branch: event.branch.clone(),
                    started_at: None,
                    started_by: None,
                    closed_at: None,
                    closed_by: None,
                    comments_count: 0,
                    last_comment_at: None,
                },
            );
        }
        TaskEventType::TaskStarted => {
            let record = map.get_mut(&event.task_id).ok_or_else(|| {
                Error::InvalidArgument(format!("task not found: {}", event.task_id))
            })?;
            let status = event
                .status
                .clone()
                .unwrap_or_else(|| config.in_progress_status.clone());
            if !config.statuses.iter().any(|value| value == &status) {
                return Err(Error::InvalidArgument(format!(
                    "unknown task status '{status}'"
                )));
            }
            record.status = status;
            record.workspace_id = event.workspace_id.clone().or(record.workspace_id.clone());
            record.workspace = event.workspace.clone().or(record.workspace.clone());
            record.branch = event.branch.clone().or(record.branch.clone());
            record.started_at = Some(event.timestamp);
            record.started_by = event.actor.clone();
            record.updated_at = event.timestamp;
            record.updated_by = event.actor.clone();
        }
        TaskEventType::TaskStatusChanged => {
            let record = map.get_mut(&event.task_id).ok_or_else(|| {
                Error::InvalidArgument(format!("task not found: {}", event.task_id))
            })?;
            let status = event
                .status
                .clone()
                .ok_or_else(|| Error::InvalidArgument("missing status".to_string()))?;
            if !config.statuses.iter().any(|value| value == &status) {
                return Err(Error::InvalidArgument(format!(
                    "unknown task status '{status}'"
                )));
            }
            record.status = status.clone();
            if config.closed_statuses.iter().any(|s| s == &status) {
                record.closed_at = Some(event.timestamp);
                record.closed_by = event.actor.clone();
            } else {
                record.closed_at = None;
                record.closed_by = None;
            }
            record.workspace_id = event.workspace_id.clone().or(record.workspace_id.clone());
            record.workspace = event.workspace.clone().or(record.workspace.clone());
            record.branch = event.branch.clone().or(record.branch.clone());
            record.updated_at = event.timestamp;
            record.updated_by = event.actor.clone();
        }
        TaskEventType::TaskClosed => {
            let record = map.get_mut(&event.task_id).ok_or_else(|| {
                Error::InvalidArgument(format!("task not found: {}", event.task_id))
            })?;
            let status = event
                .status
                .clone()
                .unwrap_or_else(|| config.closed_statuses.first().cloned().unwrap_or_else(|| "closed".to_string()));
            if !config.statuses.iter().any(|value| value == &status) {
                return Err(Error::InvalidArgument(format!(
                    "unknown task status '{status}'"
                )));
            }
            record.status = status;
            record.closed_at = Some(event.timestamp);
            record.closed_by = event.actor.clone();
            record.workspace_id = event.workspace_id.clone().or(record.workspace_id.clone());
            record.workspace = event.workspace.clone().or(record.workspace.clone());
            record.branch = event.branch.clone().or(record.branch.clone());
            record.updated_at = event.timestamp;
            record.updated_by = event.actor.clone();
        }
        TaskEventType::TaskCommented => {
            let record = map.get_mut(&event.task_id).ok_or_else(|| {
                Error::InvalidArgument(format!("task not found: {}", event.task_id))
            })?;
            record.comments_count += 1;
            record.last_comment_at = Some(event.timestamp);
            record.updated_at = event.timestamp;
            record.updated_by = event.actor.clone();
        }
    }

    Ok(())
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn strip_prefix_normalized<'a>(value_norm: &'a str, prefix_norm: &str) -> Option<&'a str> {
    for delim in TASK_ID_DELIMS {
        let full = format!("{prefix_norm}{delim}");
        if let Some(stripped) = value_norm.strip_prefix(&full) {
            return Some(stripped);
        }
    }
    None
}

fn extract_suffix_normalized<'a>(id_norm: &'a str, prefix_norm: &str) -> &'a str {
    strip_prefix_normalized(id_norm, prefix_norm).unwrap_or(id_norm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn default_config() -> TasksConfig {
        TasksConfig::default()
    }

    #[test]
    fn apply_event_builds_snapshot() {
        let config = default_config();
        let mut map = HashMap::new();
        let mut create = TaskEvent::new(TaskEventType::TaskCreated, "task-1");
        create.title = Some("Test".to_string());
        apply_event(&mut map, &create, &config).expect("create");

        let mut start = TaskEvent::new(TaskEventType::TaskStarted, "task-1");
        start.workspace = Some("ws1".to_string());
        apply_event(&mut map, &start, &config).expect("start");

        let mut comment = TaskEvent::new(TaskEventType::TaskCommented, "task-1");
        comment.comment = Some("note".to_string());
        apply_event(&mut map, &comment, &config).expect("comment");

        let task = map.get("task-1").expect("task");
        assert_eq!(task.status, "in_progress");
        assert_eq!(task.comments_count, 1);
        assert_eq!(task.workspace.as_deref(), Some("ws1"));
    }

    #[test]
    fn compact_removes_intermediate_statuses() {
        let config = default_config();
        let storage = Storage::for_repo(PathBuf::from("."));
        let store = TaskStore::new(storage, config.clone());

        let mut events = Vec::new();
        let now = Utc::now();
        let mut create = TaskEvent::new(TaskEventType::TaskCreated, "task-1");
        create.title = Some("Test".to_string());
        create.timestamp = now;
        events.push(create);

        let mut status1 = TaskEvent::new(TaskEventType::TaskStatusChanged, "task-1");
        status1.status = Some("in_progress".to_string());
        status1.timestamp = now + chrono::Duration::milliseconds(1);
        events.push(status1);

        let mut status2 = TaskEvent::new(TaskEventType::TaskClosed, "task-1");
        status2.status = Some("closed".to_string());
        status2.timestamp = now + chrono::Duration::milliseconds(2);
        events.push(status2);

        let policy = CompactionPolicy {
            older_than: None,
            max_log_mb: None,
        };
        let (compacted, report) = store.compact_events(&events, policy).expect("compact");
        assert!(compacted.len() < events.len());
        assert_eq!(report.compacted_tasks, 1);
    }

    #[test]
    fn resolve_task_id_accepts_partial_and_prefixed() {
        let dir = tempdir().expect("tempdir");
        let repo_root = dir.path().to_path_buf();
        let storage = Storage::new(
            repo_root.clone(),
            repo_root.join(".git"),
            repo_root.clone(),
        );
        let mut config = TasksConfig::default();
        config.id_prefix = "prefix".to_string();
        let store = TaskStore::new(storage, config);

        let snapshot = TaskSnapshot {
            schema_version: TASKS_SCHEMA_VERSION.to_string(),
            generated_at: Utc::now(),
            tasks: vec![
                TaskRecord {
                    id: "prefix-ab1".to_string(),
                    title: "One".to_string(),
                    status: "open".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    created_by: None,
                    updated_by: None,
                    body: None,
                    workspace_id: None,
                    workspace: None,
                    branch: None,
                    started_at: None,
                    started_by: None,
                    closed_at: None,
                    closed_by: None,
                    comments_count: 0,
                    last_comment_at: None,
                },
                TaskRecord {
                    id: "prefix-b1c".to_string(),
                    title: "Two".to_string(),
                    status: "open".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    created_by: None,
                    updated_by: None,
                    body: None,
                    workspace_id: None,
                    workspace: None,
                    branch: None,
                    started_at: None,
                    started_by: None,
                    closed_at: None,
                    closed_by: None,
                    comments_count: 0,
                    last_comment_at: None,
                },
                TaskRecord {
                    id: "prefix-a9b".to_string(),
                    title: "Three".to_string(),
                    status: "open".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    created_by: None,
                    updated_by: None,
                    body: None,
                    workspace_id: None,
                    workspace: None,
                    branch: None,
                    started_at: None,
                    started_by: None,
                    closed_at: None,
                    closed_by: None,
                    comments_count: 0,
                    last_comment_at: None,
                },
            ],
        };
        store
            .storage
            .write_json(&store.tracked_snapshot_path(), &snapshot)
            .expect("write snapshot");

        assert_eq!(
            store.resolve_task_id("ab").expect("resolve"),
            "prefix-ab1"
        );
        assert_eq!(
            store.resolve_task_id("AB").expect("resolve"),
            "prefix-ab1"
        );
        assert_eq!(
            store.resolve_task_id("b").expect("resolve"),
            "prefix-b1c"
        );
        assert_eq!(
            store.resolve_task_id("a9").expect("resolve"),
            "prefix-a9b"
        );
        assert_eq!(
            store.resolve_task_id("prefix-ab1").expect("resolve"),
            "prefix-ab1"
        );
        assert_eq!(
            store.resolve_task_id("prefix/ab1").expect("resolve"),
            "prefix-ab1"
        );
        assert_eq!(
            store.resolve_task_id("PREFIX/a9b").expect("resolve"),
            "prefix-a9b"
        );

        let err = store.resolve_task_id("a").expect_err("ambiguous");
        assert!(matches!(err, Error::InvalidArgument(_)));
    }
}
