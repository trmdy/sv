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
const TASK_ID_DELIMS: [&str; 2] = ["-", "/"];
const ULID_TIME_LEN: usize = 10;
const ULID_RANDOM_LEN: usize = 16;
const ULID_CHARSET: &str = "0123456789abcdefghjkmnpqrstvwxyz";
const ULID_CHARSET_LEN: u128 = 32;
const DEFAULT_TASK_PRIORITY: &str = "P2";
const TASK_PRIORITIES: [&str; 5] = ["P0", "P1", "P2", "P3", "P4"];

fn default_task_priority() -> String {
    DEFAULT_TASK_PRIORITY.to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskEventType {
    TaskCreated,
    TaskStarted,
    TaskStatusChanged,
    TaskPriorityChanged,
    TaskClosed,
    TaskCommented,
    TaskParentSet,
    TaskParentCleared,
    TaskBlocked,
    TaskUnblocked,
    TaskRelated,
    TaskUnrelated,
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
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_description: Option<String>,
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
            priority: None,
            workspace_id: None,
            workspace: None,
            branch: None,
            comment: None,
            related_task_id: None,
            relation_description: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default = "default_task_priority")]
    pub priority: String,
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

pub fn sort_tasks(
    tasks: &mut [TaskRecord],
    config: &TasksConfig,
    blocked_ids: &HashSet<String>,
) {
    let ready_status = config.default_status.as_str();
    tasks.sort_by(|left, right| {
        let left_status = status_rank(&left.status, config);
        let right_status = status_rank(&right.status, config);
        let left_priority = priority_rank(&left.priority);
        let right_priority = priority_rank(&right.priority);
        let left_ready = readiness_rank(left, ready_status, blocked_ids);
        let right_ready = readiness_rank(right, ready_status, blocked_ids);
        left_status
            .cmp(&right_status)
            .then_with(|| left_priority.cmp(&right_priority))
            .then_with(|| left_ready.cmp(&right_ready))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn status_rank(status: &str, config: &TasksConfig) -> usize {
    let trimmed = status.trim();
    config
        .statuses
        .iter()
        .position(|entry| entry.eq_ignore_ascii_case(trimmed))
        .unwrap_or(config.statuses.len())
}

fn priority_rank(priority: &str) -> usize {
    let trimmed = priority.trim();
    TASK_PRIORITIES
        .iter()
        .position(|entry| entry.eq_ignore_ascii_case(trimmed))
        .unwrap_or(TASK_PRIORITIES.len())
}

fn readiness_rank(task: &TaskRecord, ready_status: &str, blocked_ids: &HashSet<String>) -> usize {
    if task.status == ready_status && !blocked_ids.contains(&task.id) {
        0
    } else {
        1
    }
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TaskRelationLink {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TaskRelations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relates: Vec<TaskRelationLink>,
}

impl TaskRelations {
    fn is_empty(relations: &TaskRelations) -> bool {
        relations.parent.is_none()
            && relations.children.is_empty()
            && relations.blocks.is_empty()
            && relations.blocked_by.is_empty()
            && relations.relates.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDetails {
    pub task: TaskRecord,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<TaskComment>,
    #[serde(skip_serializing_if = "TaskRelations::is_empty")]
    pub relations: TaskRelations,
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

    pub fn list_with_ready(&self) -> Result<(Vec<TaskRecord>, HashSet<String>)> {
        let snapshot = self.load_snapshot_prefer_shared()?;
        let tasks = snapshot.tasks;
        let blocked_by = self.blocked_task_ids()?;
        let ready_status = self.config.default_status.as_str();
        let ready_ids = tasks
            .iter()
            .filter(|task| task.status == ready_status && !blocked_by.contains(&task.id))
            .map(|task| task.id.clone())
            .collect();
        Ok((tasks, ready_ids))
    }

    pub fn list_ready(&self) -> Result<Vec<TaskRecord>> {
        let (mut tasks, ready_ids) = self.list_with_ready()?;
        tasks.retain(|task| ready_ids.contains(&task.id));
        Ok(tasks)
    }

    pub fn blocked_task_ids(&self) -> Result<HashSet<String>> {
        let events = self.load_merged_events()?;
        let state = build_relation_state(&events)?;
        Ok(state
            .blocks
            .into_iter()
            .map(|(_, blocked)| blocked)
            .collect())
    }

    pub fn blocked_and_parents(&self) -> Result<(HashSet<String>, HashMap<String, String>)> {
        let events = self.load_merged_events()?;
        let state = build_relation_state(&events)?;
        let blocked = state
            .blocks
            .into_iter()
            .map(|(_, blocked)| blocked)
            .collect();
        Ok((blocked, state.parent_by_child))
    }

    fn unique_task_suffix_from_base(
        base: &str,
        len: usize,
        existing_suffixes: &HashSet<String>,
    ) -> Option<String> {
        let base = base.to_lowercase();
        let random_end = ULID_TIME_LEN + ULID_RANDOM_LEN;
        if base.len() < random_end || len == 0 || len > ULID_RANDOM_LEN {
            return None;
        }
        let random_part = &base[ULID_TIME_LEN..random_end];
        let candidate = &random_part[..len];
        if existing_suffixes.contains(candidate) {
            return None;
        }
        Some(candidate.to_string())
    }

    fn select_task_suffix_len(min_len: usize, ulid_suffix_counts: &HashMap<usize, usize>) -> usize {
        let mut len = min_len;
        loop {
            let used = ulid_suffix_counts.get(&len).copied().unwrap_or(0) as u128;
            let space = ulid_space_for_len(len);
            if used >= space && len < ULID_RANDOM_LEN {
                len += 1;
                continue;
            }
            return len;
        }
    }

    pub fn generate_task_id(&self) -> Result<String> {
        let prefix = self.config.id_prefix.trim();
        let snapshot = self.snapshot_readonly()?;
        let mut existing_suffixes = HashSet::new();
        let mut ulid_suffix_counts: HashMap<usize, usize> = HashMap::new();
        for task in snapshot.tasks {
            let id_norm = normalize_id(&task.id);
            let suffix = suffix_from_id(&id_norm);
            if suffix.is_empty() {
                continue;
            }
            existing_suffixes.insert(suffix.to_string());
            if is_ulid_suffix(suffix) {
                *ulid_suffix_counts.entry(suffix.len()).or_insert(0) += 1;
            }
        }

        let min_len = self.config.id_min_len;
        let target_len = Self::select_task_suffix_len(min_len, &ulid_suffix_counts);

        loop {
            let base = Ulid::new().to_string();
            if let Some(suffix) =
                Self::unique_task_suffix_from_base(&base, target_len, &existing_suffixes)
            {
                return Ok(format!("{}-{}", prefix, suffix));
            }
        }
    }

    pub fn resolve_task_id(&self, input: &str) -> Result<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument("task id cannot be empty".to_string()));
        }

        let trimmed_norm = normalize_id(trimmed);
        let candidate_norm = suffix_from_id(&trimmed_norm).to_string();
        if candidate_norm.is_empty() {
            return Err(Error::InvalidArgument("task id cannot be empty".to_string()));
        }

        let snapshot = self.snapshot_readonly()?;
        let mut exact: Vec<String> = Vec::new();
        let mut matches: Vec<String> = Vec::new();

        for task in snapshot.tasks {
            let id_norm = normalize_id(&task.id);
            let suffix_norm = suffix_from_id(&id_norm);
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
            .iter()
            .filter(|event| event.task_id == task_id)
            .cloned()
            .collect();
        if filtered.is_empty() {
            return Err(Error::InvalidArgument(format!(
                "task not found: {task_id}"
            )));
        }
        sort_events(&mut filtered);
        let snapshot = self.build_snapshot(&events)?;
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
        let relations = build_relations(task_id, &events)?;
        let events_count = events
            .iter()
            .filter(|event| {
                event.task_id == task_id || event.related_task_id.as_deref() == Some(task_id)
            })
            .count();
        Ok(TaskDetails {
            task,
            comments,
            relations,
            events: events_count,
        })
    }

    pub fn relations(&self, task_id: &str) -> Result<TaskRelations> {
        let events = self.load_merged_events()?;
        build_relations(task_id, &events)
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

            for event in task_events
                .iter()
                .filter(|event| is_relation_event(event.event_type))
            {
                keep_ids.insert(event.event_id.clone());
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

            if let Some(last_priority) = task_events
                .iter()
                .rev()
                .find(|event| event.event_type == TaskEventType::TaskPriorityChanged)
            {
                keep_ids.insert(last_priority.event_id.clone());
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

    pub fn normalize_priority(&self, priority: &str) -> Result<String> {
        normalize_priority(priority)
    }

    pub fn default_priority(&self) -> String {
        default_task_priority()
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

fn normalize_priority(priority: &str) -> Result<String> {
    let trimmed = priority.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidArgument("priority cannot be empty".to_string()));
    }

    let normalized = trimmed.to_ascii_uppercase();
    if TASK_PRIORITIES.iter().any(|value| value == &normalized) {
        Ok(normalized)
    } else {
        Err(Error::InvalidArgument(format!(
            "unknown task priority '{trimmed}' (expected P0-P4)"
        )))
    }
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
        TaskEventType::TaskCommented
        | TaskEventType::TaskPriorityChanged
        | TaskEventType::TaskParentSet
        | TaskEventType::TaskParentCleared
        | TaskEventType::TaskBlocked
        | TaskEventType::TaskUnblocked
        | TaskEventType::TaskRelated
        | TaskEventType::TaskUnrelated => return Ok(None),
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
            let priority = match event.priority.as_deref() {
                Some(value) => normalize_priority(value)?,
                None => default_task_priority(),
            };

            let now = event.timestamp;
            map.insert(
                event.task_id.clone(),
                TaskRecord {
                    id: event.task_id.clone(),
                    title,
                    status,
                    priority,
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
        TaskEventType::TaskPriorityChanged => {
            let record = map.get_mut(&event.task_id).ok_or_else(|| {
                Error::InvalidArgument(format!("task not found: {}", event.task_id))
            })?;
            let priority = event
                .priority
                .as_deref()
                .ok_or_else(|| Error::InvalidArgument("missing priority".to_string()))?;
            record.priority = normalize_priority(priority)?;
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
        TaskEventType::TaskParentSet
        | TaskEventType::TaskParentCleared
        | TaskEventType::TaskBlocked
        | TaskEventType::TaskUnblocked
        | TaskEventType::TaskRelated
        | TaskEventType::TaskUnrelated => {
            let Some(related_task_id) = relation_target(event) else {
                return Ok(());
            };
            if related_task_id == event.task_id {
                return Ok(());
            }
            if event.event_type == TaskEventType::TaskRelated && relation_description(event).is_none()
            {
                return Ok(());
            }
            touch_task_if_present(map, &event.task_id, event);
            touch_task_if_present(map, related_task_id, event);
        }
    }

    Ok(())
}

fn touch_task_if_present(map: &mut HashMap<String, TaskRecord>, task_id: &str, event: &TaskEvent) {
    if let Some(record) = map.get_mut(task_id) {
        record.updated_at = event.timestamp;
        record.updated_by = event.actor.clone();
    }
}

fn relation_target(event: &TaskEvent) -> Option<&str> {
    let target = event.related_task_id.as_deref()?;
    let trimmed = target.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn relation_description(event: &TaskEvent) -> Option<&str> {
    let description = event.relation_description.as_deref()?;
    let trimmed = description.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn is_relation_event(event_type: TaskEventType) -> bool {
    matches!(
        event_type,
        TaskEventType::TaskParentSet
            | TaskEventType::TaskParentCleared
            | TaskEventType::TaskBlocked
            | TaskEventType::TaskUnblocked
            | TaskEventType::TaskRelated
            | TaskEventType::TaskUnrelated
    )
}

#[derive(Default)]
struct RelationState {
    parent_by_child: HashMap<String, String>,
    blocks: HashSet<(String, String)>,
    relates: HashMap<(String, String), String>,
}

fn relation_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn apply_relation_event(state: &mut RelationState, event: &TaskEvent) -> Result<()> {
    if !is_relation_event(event.event_type) {
        return Ok(());
    }
    let Some(related_task_id) = relation_target(event) else {
        return Ok(());
    };
    if related_task_id == event.task_id {
        return Ok(());
    }

    match event.event_type {
        TaskEventType::TaskParentSet => {
            state
                .parent_by_child
                .insert(event.task_id.clone(), related_task_id.to_string());
        }
        TaskEventType::TaskParentCleared => {
            if let Some(current) = state.parent_by_child.get(&event.task_id) {
                if current == related_task_id {
                    state.parent_by_child.remove(&event.task_id);
                }
            }
        }
        TaskEventType::TaskBlocked => {
            state
                .blocks
                .insert((event.task_id.clone(), related_task_id.to_string()));
        }
        TaskEventType::TaskUnblocked => {
            state
                .blocks
                .remove(&(event.task_id.clone(), related_task_id.to_string()));
        }
        TaskEventType::TaskRelated => {
            let Some(description) = relation_description(event) else {
                return Ok(());
            };
            let key = relation_key(&event.task_id, related_task_id);
            state.relates.insert(key, description.to_string());
        }
        TaskEventType::TaskUnrelated => {
            let key = relation_key(&event.task_id, related_task_id);
            state.relates.remove(&key);
        }
        _ => {}
    }

    Ok(())
}

fn build_relation_state(events: &[TaskEvent]) -> Result<RelationState> {
    let mut sorted = events.to_vec();
    sort_events(&mut sorted);
    let mut state = RelationState::default();
    for event in &sorted {
        apply_relation_event(&mut state, event)?;
    }
    Ok(state)
}

fn build_relations(task_id: &str, events: &[TaskEvent]) -> Result<TaskRelations> {
    if !events.iter().any(|event| event.task_id == task_id) {
        return Err(Error::InvalidArgument(format!(
            "task not found: {task_id}"
        )));
    }
    let state = build_relation_state(events)?;

    let parent = state.parent_by_child.get(task_id).cloned();
    let mut children: Vec<String> = state
        .parent_by_child
        .iter()
        .filter_map(|(child, parent)| {
            if parent == task_id {
                Some(child.clone())
            } else {
                None
            }
        })
        .collect();
    let mut blocks: Vec<String> = state
        .blocks
        .iter()
        .filter_map(|(blocker, blocked)| {
            if blocker == task_id {
                Some(blocked.clone())
            } else {
                None
            }
        })
        .collect();
    let mut blocked_by: Vec<String> = state
        .blocks
        .iter()
        .filter_map(|(blocker, blocked)| {
            if blocked == task_id {
                Some(blocker.clone())
            } else {
                None
            }
        })
        .collect();
    let mut relates: Vec<TaskRelationLink> = state
        .relates
        .iter()
        .filter_map(|((left, right), description)| {
            if left == task_id {
                Some(TaskRelationLink {
                    id: right.clone(),
                    description: description.clone(),
                })
            } else if right == task_id {
                Some(TaskRelationLink {
                    id: left.clone(),
                    description: description.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    children.sort();
    blocks.sort();
    blocked_by.sort();
    relates.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.description.cmp(&b.description)));

    Ok(TaskRelations {
        parent,
        children,
        blocks,
        blocked_by,
        relates,
    })
}

#[cfg(test)]
fn blocked_task_ids_from_events(events: &[TaskEvent]) -> Result<HashSet<String>> {
    let state = build_relation_state(events)?;
    Ok(state.blocks.into_iter().map(|(_, blocked)| blocked).collect())
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn suffix_from_id<'a>(id_norm: &'a str) -> &'a str {
    let mut earliest = None;
    for delim in TASK_ID_DELIMS {
        if let Some(idx) = id_norm.find(delim) {
            earliest = match earliest {
                Some(current) => Some(std::cmp::min(current, idx)),
                None => Some(idx),
            };
        }
    }
    if let Some(idx) = earliest {
        let start = idx + 1;
        if start < id_norm.len() {
            &id_norm[start..]
        } else {
            ""
        }
    } else {
        id_norm
    }
}

fn is_ulid_suffix(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ULID_CHARSET.contains(ch))
}

fn ulid_space_for_len(len: usize) -> u128 {
    let mut space = 1u128;
    for _ in 0..len {
        space *= ULID_CHARSET_LEN;
    }
    space
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
        assert_eq!(task.priority, DEFAULT_TASK_PRIORITY);
        assert_eq!(task.comments_count, 1);
        assert_eq!(task.workspace.as_deref(), Some("ws1"));
    }

    #[test]
    fn priority_changes_update_record() {
        let config = default_config();
        let mut map = HashMap::new();
        let mut create = TaskEvent::new(TaskEventType::TaskCreated, "task-1");
        create.title = Some("Test".to_string());
        create.priority = Some("P1".to_string());
        apply_event(&mut map, &create, &config).expect("create");

        let mut change = TaskEvent::new(TaskEventType::TaskPriorityChanged, "task-1");
        change.priority = Some("p0".to_string());
        apply_event(&mut map, &change, &config).expect("priority");

        let task = map.get("task-1").expect("task");
        assert_eq!(task.priority, "P0");
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
    fn relations_include_parent_and_children() {
        let now = Utc::now();
        let mut events = Vec::new();
        let mut parent = TaskEvent::new(TaskEventType::TaskCreated, "task-parent");
        parent.title = Some("Parent".to_string());
        parent.timestamp = now;
        events.push(parent);

        let mut child = TaskEvent::new(TaskEventType::TaskCreated, "task-child");
        child.title = Some("Child".to_string());
        child.timestamp = now + chrono::Duration::milliseconds(1);
        events.push(child);

        let mut set_parent = TaskEvent::new(TaskEventType::TaskParentSet, "task-child");
        set_parent.related_task_id = Some("task-parent".to_string());
        set_parent.timestamp = now + chrono::Duration::milliseconds(2);
        events.push(set_parent);

        let child_relations = build_relations("task-child", &events).expect("relations");
        assert_eq!(child_relations.parent.as_deref(), Some("task-parent"));
        assert!(child_relations.children.is_empty());

        let parent_relations = build_relations("task-parent", &events).expect("relations");
        assert_eq!(parent_relations.children, vec!["task-child".to_string()]);
    }

    #[test]
    fn relations_include_blocks_and_relates() {
        let now = Utc::now();
        let mut events = Vec::new();
        for id in ["task-a", "task-b", "task-c"] {
            let mut create = TaskEvent::new(TaskEventType::TaskCreated, id);
            create.title = Some(id.to_string());
            create.timestamp = now;
            events.push(create);
        }

        let mut block = TaskEvent::new(TaskEventType::TaskBlocked, "task-a");
        block.related_task_id = Some("task-b".to_string());
        block.timestamp = now + chrono::Duration::milliseconds(1);
        events.push(block);

        let mut relate = TaskEvent::new(TaskEventType::TaskRelated, "task-a");
        relate.related_task_id = Some("task-c".to_string());
        relate.relation_description = Some("shares context".to_string());
        relate.timestamp = now + chrono::Duration::milliseconds(2);
        events.push(relate);

        let relations_a = build_relations("task-a", &events).expect("relations");
        assert_eq!(relations_a.blocks, vec!["task-b".to_string()]);
        assert_eq!(
            relations_a.relates,
            vec![TaskRelationLink {
                id: "task-c".to_string(),
                description: "shares context".to_string(),
            }]
        );

        let relations_b = build_relations("task-b", &events).expect("relations");
        assert_eq!(relations_b.blocked_by, vec!["task-a".to_string()]);

        let relations_c = build_relations("task-c", &events).expect("relations");
        assert_eq!(
            relations_c.relates,
            vec![TaskRelationLink {
                id: "task-a".to_string(),
                description: "shares context".to_string(),
            }]
        );
    }

    #[test]
    fn blocked_task_ids_respects_unblocked_events() {
        let now = Utc::now();
        let mut events = Vec::new();
        for id in ["task-a", "task-b", "task-c"] {
            let mut create = TaskEvent::new(TaskEventType::TaskCreated, id);
            create.title = Some(id.to_string());
            create.timestamp = now;
            events.push(create);
        }

        let mut block_ab = TaskEvent::new(TaskEventType::TaskBlocked, "task-a");
        block_ab.related_task_id = Some("task-b".to_string());
        block_ab.timestamp = now + chrono::Duration::milliseconds(1);
        events.push(block_ab);

        let mut block_ca = TaskEvent::new(TaskEventType::TaskBlocked, "task-c");
        block_ca.related_task_id = Some("task-a".to_string());
        block_ca.timestamp = now + chrono::Duration::milliseconds(2);
        events.push(block_ca);

        let mut unblock_ab = TaskEvent::new(TaskEventType::TaskUnblocked, "task-a");
        unblock_ab.related_task_id = Some("task-b".to_string());
        unblock_ab.timestamp = now + chrono::Duration::milliseconds(3);
        events.push(unblock_ab);

        let blocked = blocked_task_ids_from_events(&events).expect("blocked");
        assert!(blocked.contains("task-a"));
        assert!(!blocked.contains("task-b"));
    }

    #[test]
    fn relation_events_missing_targets_are_ignored() {
        let config = default_config();
        let mut map = HashMap::new();
        let mut create = TaskEvent::new(TaskEventType::TaskCreated, "task-1");
        create.title = Some("Test".to_string());
        apply_event(&mut map, &create, &config).expect("create");

        let mut relate = TaskEvent::new(TaskEventType::TaskRelated, "task-1");
        relate.relation_description = Some("context".to_string());
        apply_event(&mut map, &relate, &config).expect("relation");

        let task = map.get("task-1").expect("task");
        assert_eq!(task.title, "Test");
    }

    #[test]
    fn relations_ignore_missing_related_task_id() {
        let now = Utc::now();
        let mut events = Vec::new();
        let mut create = TaskEvent::new(TaskEventType::TaskCreated, "task-a");
        create.title = Some("A".to_string());
        create.timestamp = now;
        events.push(create);

        let mut relate = TaskEvent::new(TaskEventType::TaskRelated, "task-a");
        relate.relation_description = Some("context".to_string());
        relate.timestamp = now + chrono::Duration::milliseconds(1);
        events.push(relate);

        let relations = build_relations("task-a", &events).expect("relations");
        assert!(relations.relates.is_empty());
    }

    #[test]
    fn list_ready_excludes_blocked_tasks() {
        let dir = tempdir().expect("tempdir");
        let repo_root = dir.path().to_path_buf();
        let storage = Storage::new(
            repo_root.clone(),
            repo_root.join(".git"),
            repo_root.clone(),
        );
        let store = TaskStore::new(storage, TasksConfig::default());

        let now = Utc::now();
        let mut create_a = TaskEvent::new(TaskEventType::TaskCreated, "task-a");
        create_a.title = Some("A".to_string());
        create_a.timestamp = now;
        store.append_event(create_a).expect("create a");

        let mut create_b = TaskEvent::new(TaskEventType::TaskCreated, "task-b");
        create_b.title = Some("B".to_string());
        create_b.timestamp = now + chrono::Duration::milliseconds(1);
        store.append_event(create_b).expect("create b");

        let mut block = TaskEvent::new(TaskEventType::TaskBlocked, "task-a");
        block.related_task_id = Some("task-b".to_string());
        block.timestamp = now + chrono::Duration::milliseconds(2);
        store.append_event(block).expect("block");

        let ready = store.list_ready().expect("list ready");
        let ids: HashSet<String> = ready.into_iter().map(|task| task.id).collect();
        assert!(ids.contains("task-a"));
        assert!(!ids.contains("task-b"));
    }

    #[test]
    fn task_id_suffix_uses_random_section() {
        let existing = HashSet::new();
        let suffix = TaskStore::unique_task_suffix_from_base(
            "0123456789abcdefghijklmnop",
            3,
            &existing,
        )
        .expect("suffix");
        assert_eq!(suffix, "abc");
    }

    #[test]
    fn task_id_suffix_stays_same_length_when_taken() {
        let mut existing = HashSet::new();
        existing.insert("abc".to_string());
        let suffix = TaskStore::unique_task_suffix_from_base(
            "0123456789abcdefghijklmnop",
            3,
            &existing,
        );
        assert!(suffix.is_none());
    }

    #[test]
    fn task_id_suffix_length_stays_min_until_exhausted() {
        let mut counts = HashMap::new();
        counts.insert(3, 1);
        assert_eq!(TaskStore::select_task_suffix_len(3, &counts), 3);
    }

    #[test]
    fn task_id_suffix_length_grows_after_exhausted() {
        let mut counts = HashMap::new();
        counts.insert(3, ulid_space_for_len(3) as usize);
        assert_eq!(TaskStore::select_task_suffix_len(3, &counts), 4);
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
                    id: "old-ab1".to_string(),
                    title: "One".to_string(),
                    status: "open".to_string(),
                    priority: default_task_priority(),
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
                    priority: default_task_priority(),
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
                    id: "legacy-a9b".to_string(),
                    title: "Three".to_string(),
                    status: "open".to_string(),
                    priority: default_task_priority(),
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
            "old-ab1"
        );
        assert_eq!(
            store.resolve_task_id("AB").expect("resolve"),
            "old-ab1"
        );
        assert_eq!(
            store.resolve_task_id("b").expect("resolve"),
            "prefix-b1c"
        );
        assert_eq!(
            store.resolve_task_id("a9").expect("resolve"),
            "legacy-a9b"
        );
        assert_eq!(
            store.resolve_task_id("old-ab1").expect("resolve"),
            "old-ab1"
        );
        assert_eq!(
            store.resolve_task_id("old/ab1").expect("resolve"),
            "old-ab1"
        );
        assert_eq!(
            store.resolve_task_id("LEGACY/a9b").expect("resolve"),
            "legacy-a9b"
        );

        let err = store.resolve_task_id("a").expect_err("ambiguous");
        assert!(matches!(err, Error::InvalidArgument(_)));
    }
}
