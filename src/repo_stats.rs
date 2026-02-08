use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::error::Result;
use crate::project::{ProjectEvent, ProjectStore};
use crate::task::{CompactionPolicy, TaskEvent, TaskEventType, TaskRecord, TaskStore};

#[derive(Debug, Clone, Serialize)]
pub struct StatusCount {
    pub status: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThroughputWindow {
    pub window_hours: u32,
    pub tasks_created: usize,
    pub tasks_completed: usize,
    pub created_per_hour: f64,
    pub completed_per_hour: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactionEstimate {
    pub before_events: usize,
    pub after_events: usize,
    pub removable_events: usize,
    pub estimated_bytes_saved: u64,
    pub estimated_percent_saved: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoStats {
    pub generated_at: DateTime<Utc>,
    pub tasks_total: usize,
    pub ready_tasks: usize,
    pub blocked_tasks: usize,
    pub tasks_with_workspace: usize,
    pub epics_total: usize,
    pub project_groups_total: usize,
    pub project_entities_total: usize,
    pub project_entities_active: usize,
    pub project_entities_archived: usize,
    pub task_statuses: Vec<StatusCount>,
    pub epic_statuses: Vec<StatusCount>,
    pub project_group_statuses: Vec<StatusCount>,
    pub events_total: usize,
    pub task_events_total: usize,
    pub project_events_total: usize,
    pub disk_usage_bytes: u64,
    pub task_log_bytes: u64,
    pub project_log_bytes: u64,
    pub compaction: CompactionEstimate,
    pub throughput_last_hour: ThroughputWindow,
    pub throughput_last_24_hours: ThroughputWindow,
    pub avg_task_events: f64,
}

pub fn compute(task_store: &TaskStore, project_store: &ProjectStore) -> Result<RepoStats> {
    let generated_at = Utc::now();

    let snapshot = task_store.snapshot_readonly()?;
    let tasks = snapshot.tasks;
    let tasks_total = tasks.len();

    let blocked_ids = task_store.blocked_task_ids()?;
    let blocked_tasks = blocked_ids.len();
    let ready_status = task_store.config().default_status.as_str();
    let ready_tasks = tasks
        .iter()
        .filter(|task| task.status == ready_status && !blocked_ids.contains(&task.id))
        .count();

    let tasks_with_workspace = tasks
        .iter()
        .filter(|task| {
            task.workspace
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty())
        })
        .count();

    let epic_ids = collect_epic_ids(&tasks);
    let epics_total = epic_ids.len();

    let task_statuses = count_statuses(tasks.iter().map(|task| task.status.as_str()));
    let epic_statuses = count_statuses(
        tasks
            .iter()
            .filter(|task| epic_ids.contains(&task.id))
            .map(|task| task.status.as_str()),
    );

    let effective_project_ids = effective_project_ids(&tasks);
    let mut project_groups: HashSet<String> = HashSet::new();
    for project in &effective_project_ids {
        if let Some(project_id) = project.as_ref() {
            project_groups.insert(project_id.clone());
        }
    }
    let project_groups_total = project_groups.len();

    let mut task_by_id: HashMap<&str, &TaskRecord> = HashMap::new();
    for task in &tasks {
        task_by_id.insert(task.id.as_str(), task);
    }
    let project_group_statuses = count_statuses(project_groups.iter().map(|project_id| {
        task_by_id
            .get(project_id.as_str())
            .map(|task| task.status.as_str())
            .unwrap_or("external")
    }));

    let project_snapshot = project_store.snapshot_readonly()?;
    let project_entities_total = project_snapshot.projects.len();
    let project_entities_archived = project_snapshot
        .projects
        .iter()
        .filter(|project| project.archived)
        .count();
    let project_entities_active = project_entities_total.saturating_sub(project_entities_archived);

    let task_events = load_merged_task_events(task_store)?;
    let project_events = load_merged_project_events(project_store)?;
    let task_events_total = task_events.len();
    let project_events_total = project_events.len();
    let events_total = task_events_total + project_events_total;

    let task_log_bytes =
        log_file_bytes(task_store.tracked_log_path(), task_store.shared_log_path());
    let project_log_bytes = log_file_bytes(
        project_store.tracked_log_path(),
        project_store.shared_log_path(),
    );

    let disk_usage_bytes = task_storage_bytes(task_store) + project_storage_bytes(project_store);

    let (compacted_events, compact_report) = task_store.compact(CompactionPolicy {
        older_than: None,
        max_log_mb: None,
    })?;
    let task_events_bytes = jsonl_task_bytes(&task_events)?;
    let compacted_bytes = jsonl_task_bytes(&compacted_events)?;
    let estimated_bytes_saved = task_events_bytes.saturating_sub(compacted_bytes);
    let estimated_percent_saved = ratio_pct(estimated_bytes_saved as f64, task_events_bytes as f64);

    let compaction = CompactionEstimate {
        before_events: compact_report.before_events,
        after_events: compact_report.after_events,
        removable_events: compact_report.removed_events,
        estimated_bytes_saved,
        estimated_percent_saved,
    };

    let throughput_last_hour = throughput_window(&task_events, Duration::hours(1), task_store);
    let throughput_last_24_hours = throughput_window(&task_events, Duration::hours(24), task_store);

    let avg_task_events = if tasks_total == 0 {
        0.0
    } else {
        round2(task_events_total as f64 / tasks_total as f64)
    };

    Ok(RepoStats {
        generated_at,
        tasks_total,
        ready_tasks,
        blocked_tasks,
        tasks_with_workspace,
        epics_total,
        project_groups_total,
        project_entities_total,
        project_entities_active,
        project_entities_archived,
        task_statuses,
        epic_statuses,
        project_group_statuses,
        events_total,
        task_events_total,
        project_events_total,
        disk_usage_bytes,
        task_log_bytes,
        project_log_bytes,
        compaction,
        throughput_last_hour,
        throughput_last_24_hours,
        avg_task_events,
    })
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes < 1024 {
        format!("{bytes} B")
    } else if (bytes as f64) < MB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else if (bytes as f64) < GB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{:.2} GB", bytes as f64 / GB)
    }
}

fn throughput_window(
    task_events: &[TaskEvent],
    window: Duration,
    task_store: &TaskStore,
) -> ThroughputWindow {
    let now = Utc::now();
    let cutoff = now - window;
    let closed_statuses: HashSet<&str> = task_store
        .config()
        .closed_statuses
        .iter()
        .map(|status| status.as_str())
        .collect();

    let tasks_created = task_events
        .iter()
        .filter(|event| event.timestamp >= cutoff && event.event_type == TaskEventType::TaskCreated)
        .count();
    let tasks_completed = task_events
        .iter()
        .filter(|event| event.timestamp >= cutoff)
        .filter(|event| completion_event(event, &closed_statuses))
        .count();

    let hours = window.num_hours().max(1) as u32;
    ThroughputWindow {
        window_hours: hours,
        tasks_created,
        tasks_completed,
        created_per_hour: round2(tasks_created as f64 / hours as f64),
        completed_per_hour: round2(tasks_completed as f64 / hours as f64),
    }
}

fn completion_event(event: &TaskEvent, closed_statuses: &HashSet<&str>) -> bool {
    match event.event_type {
        TaskEventType::TaskClosed => true,
        TaskEventType::TaskStatusChanged => event
            .status
            .as_deref()
            .map(|status| closed_statuses.contains(status))
            .unwrap_or(false),
        _ => false,
    }
}

fn count_statuses<'a>(statuses: impl Iterator<Item = &'a str>) -> Vec<StatusCount> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for status in statuses {
        *counts.entry(status.trim().to_string()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .map(|(status, count)| StatusCount { status, count })
        .collect()
}

fn collect_epic_ids(tasks: &[TaskRecord]) -> HashSet<String> {
    let mut task_ids: HashSet<&str> = HashSet::new();
    for task in tasks {
        task_ids.insert(task.id.as_str());
    }

    let mut epic_ids = HashSet::new();
    for task in tasks {
        if let Some(epic) = task.epic.as_deref() {
            if task_ids.contains(epic) {
                epic_ids.insert(epic.to_string());
            }
        }
    }
    epic_ids
}

fn effective_project_ids(tasks: &[TaskRecord]) -> Vec<Option<String>> {
    if tasks.is_empty() {
        return Vec::new();
    }

    let mut index_by_id = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        index_by_id.insert(task.id.as_str(), idx);
    }

    let mut cache: Vec<Option<Option<String>>> = vec![None; tasks.len()];
    let mut out = Vec::with_capacity(tasks.len());
    for idx in 0..tasks.len() {
        out.push(resolve_effective_project(
            idx,
            tasks,
            &index_by_id,
            &mut cache,
            &mut HashSet::new(),
        ));
    }

    out
}

fn resolve_effective_project<'a>(
    idx: usize,
    tasks: &'a [TaskRecord],
    index_by_id: &HashMap<&'a str, usize>,
    cache: &mut [Option<Option<String>>],
    visiting: &mut HashSet<usize>,
) -> Option<String> {
    if let Some(cached) = cache.get(idx).and_then(|value| value.as_ref()) {
        return cached.clone();
    }

    if !visiting.insert(idx) {
        return tasks.get(idx).and_then(|task| task.project.clone());
    }

    let resolved = if let Some(project) = tasks.get(idx).and_then(|task| task.project.clone()) {
        Some(project)
    } else if let Some(epic_idx) = tasks
        .get(idx)
        .and_then(|task| task.epic.as_deref())
        .and_then(|epic_id| index_by_id.get(epic_id))
        .copied()
    {
        resolve_effective_project(epic_idx, tasks, index_by_id, cache, visiting)
    } else {
        None
    };

    visiting.remove(&idx);
    cache[idx] = Some(resolved.clone());
    resolved
}

fn load_merged_task_events(task_store: &TaskStore) -> Result<Vec<TaskEvent>> {
    let storage = task_store.storage();
    let tracked: Vec<TaskEvent> = if task_store.tracked_log_path().exists() {
        storage.read_jsonl(&task_store.tracked_log_path())?
    } else {
        Vec::new()
    };
    let shared: Vec<TaskEvent> = if task_store.shared_log_path().exists() {
        storage.read_jsonl(&task_store.shared_log_path())?
    } else {
        Vec::new()
    };
    Ok(merge_task_events(tracked, shared))
}

fn load_merged_project_events(project_store: &ProjectStore) -> Result<Vec<ProjectEvent>> {
    let storage = project_store.storage();
    let tracked: Vec<ProjectEvent> = if project_store.tracked_log_path().exists() {
        storage.read_jsonl(&project_store.tracked_log_path())?
    } else {
        Vec::new()
    };
    let shared: Vec<ProjectEvent> = if project_store.shared_log_path().exists() {
        storage.read_jsonl(&project_store.shared_log_path())?
    } else {
        Vec::new()
    };
    Ok(merge_project_events(tracked, shared))
}

fn merge_task_events(mut tracked: Vec<TaskEvent>, shared: Vec<TaskEvent>) -> Vec<TaskEvent> {
    let mut seen = HashSet::new();
    tracked.retain(|event| seen.insert(event.event_id.clone()));
    for event in shared {
        if seen.insert(event.event_id.clone()) {
            tracked.push(event);
        }
    }
    tracked.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    tracked
}

fn merge_project_events(
    mut tracked: Vec<ProjectEvent>,
    shared: Vec<ProjectEvent>,
) -> Vec<ProjectEvent> {
    let mut seen = HashSet::new();
    tracked.retain(|event| seen.insert(event.event_id.clone()));
    for event in shared {
        if seen.insert(event.event_id.clone()) {
            tracked.push(event);
        }
    }
    tracked.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    tracked
}

fn task_storage_bytes(task_store: &TaskStore) -> u64 {
    file_size(task_store.tracked_log_path())
        + file_size(task_store.shared_log_path())
        + file_size(task_store.tracked_snapshot_path())
        + file_size(task_store.shared_snapshot_path())
}

fn project_storage_bytes(project_store: &ProjectStore) -> u64 {
    file_size(project_store.tracked_log_path())
        + file_size(project_store.shared_log_path())
        + file_size(project_store.tracked_snapshot_path())
        + file_size(project_store.shared_snapshot_path())
}

fn log_file_bytes(tracked_path: std::path::PathBuf, shared_path: std::path::PathBuf) -> u64 {
    file_size(tracked_path) + file_size(shared_path)
}

fn file_size(path: std::path::PathBuf) -> u64 {
    path.metadata().map(|meta| meta.len()).unwrap_or(0)
}

fn jsonl_task_bytes(events: &[TaskEvent]) -> Result<u64> {
    let mut total = 0u64;
    for event in events {
        total = total.saturating_add(serde_json::to_vec(event)?.len() as u64 + 1);
    }
    Ok(total)
}

fn ratio_pct(numerator: f64, denominator: f64) -> f64 {
    if denominator <= f64::EPSILON {
        0.0
    } else {
        round2((numerator / denominator) * 100.0)
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskEvent;

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn counts_completion_events() {
        let now = Utc::now();
        let mut close = TaskEvent::new(TaskEventType::TaskClosed, "sv-a");
        close.timestamp = now;

        let mut status = TaskEvent::new(TaskEventType::TaskStatusChanged, "sv-b");
        status.status = Some("closed".to_string());
        status.timestamp = now;

        let mut open = TaskEvent::new(TaskEventType::TaskStatusChanged, "sv-c");
        open.status = Some("open".to_string());
        open.timestamp = now;

        let closed = HashSet::from(["closed"]);
        assert!(completion_event(&close, &closed));
        assert!(completion_event(&status, &closed));
        assert!(!completion_event(&open, &closed));
    }
}
