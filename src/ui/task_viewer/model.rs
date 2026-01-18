use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::config::TasksConfig;
use crate::task::TaskRecord;

fn normalize_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_status(value: &str) -> String {
    normalize_text(value)
}

pub fn sort_tasks(
    tasks: &mut [TaskRecord],
    config: &TasksConfig,
    blocked_ids: &HashSet<String>,
) {
    crate::task::sort_tasks(tasks, config, blocked_ids);
}

fn fuzzy_match(value: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut query_chars = query.chars();
    let mut current = query_chars.next();
    for ch in value.chars() {
        if Some(ch) == current {
            current = query_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}

pub fn filter_task_indices(
    tasks: &[TaskRecord],
    query: &str,
    status_filter: Option<&str>,
) -> Vec<usize> {
    let query_norm = normalize_text(query);
    let status_norm = status_filter.map(normalize_status);
    let mut indices = Vec::new();

    for (idx, task) in tasks.iter().enumerate() {
        if let Some(status) = status_norm.as_deref() {
            if normalize_status(&task.status) != status {
                continue;
            }
        }

        if query_norm.is_empty() {
            indices.push(idx);
            continue;
        }

        let id_norm = normalize_text(&task.id);
        let title_norm = normalize_text(&task.title);
        if fuzzy_match(&id_norm, &query_norm) || fuzzy_match(&title_norm, &query_norm) {
            indices.push(idx);
        }
    }

    indices
}

pub fn select_by_id(
    tasks: &[TaskRecord],
    filtered: &[usize],
    previous_id: Option<&str>,
) -> Option<usize> {
    if filtered.is_empty() {
        return None;
    }
    if let Some(id) = previous_id {
        let normalized = normalize_text(id);
        if let Some(index) = tasks.iter().position(|task| normalize_text(&task.id) == normalized) {
            if filtered.iter().any(|candidate| *candidate == index) {
                return Some(index);
            }
        }
    }
    Some(filtered[0])
}

pub fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(
        id: &str,
        title: &str,
        status: &str,
        priority: &str,
        updated_at: DateTime<Utc>,
    ) -> TaskRecord {
        TaskRecord {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            created_at: updated_at,
            updated_at,
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
        }
    }

    #[test]
    fn filter_matches_id_and_title_case_insensitive() {
        let now = Utc::now();
        let tasks = vec![
            task("sv-aaa", "Fix Sync", "open", "P2", now),
            task("sv-bbb", "Add watcher", "open", "P2", now),
        ];
        let indices = filter_task_indices(&tasks, "SYNC", None);
        assert_eq!(indices, vec![0]);

        let indices = filter_task_indices(&tasks, "watch", None);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn filter_combines_status_and_text() {
        let now = Utc::now();
        let tasks = vec![
            task("sv-aaa", "Fix Sync", "open", "P2", now),
            task("sv-bbb", "Fix Sync", "closed", "P2", now),
        ];
        let indices = filter_task_indices(&tasks, "sync", Some("open"));
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn sort_orders_by_status_priority_readiness() {
        let config = TasksConfig::default();
        let now = Utc::now();
        let earlier = now - chrono::Duration::seconds(60);
        let mut blocked_ids = HashSet::new();
        blocked_ids.insert("sv-3".to_string());
        let mut tasks = vec![
            task("sv-4", "Fourth", "closed", "P0", now),
            task("sv-1", "First", "open", "P1", earlier),
            task("sv-3", "Third", "open", "P0", now + chrono::Duration::seconds(10)),
            task("sv-2", "Second", "open", "P0", now),
        ];
        sort_tasks(&mut tasks, &config, &blocked_ids);
        assert_eq!(tasks[0].id, "sv-2");
        assert_eq!(tasks[1].id, "sv-3");
        assert_eq!(tasks[2].id, "sv-1");
        assert_eq!(tasks[3].id, "sv-4");
    }

    #[test]
    fn selection_persists_by_id_or_falls_back() {
        let now = Utc::now();
        let tasks = vec![
            task("sv-1", "One", "open", "P2", now),
            task("sv-2", "Two", "open", "P2", now),
        ];
        let filtered = vec![0, 1];
        assert_eq!(select_by_id(&tasks, &filtered, Some("sv-2")), Some(1));
        assert_eq!(select_by_id(&tasks, &filtered, Some("sv-3")), Some(0));
    }

    #[test]
    fn parse_timestamp_returns_utc() {
        let parsed = parse_timestamp("2025-01-12T12:34:56Z").expect("timestamp");
        assert_eq!(parsed.timezone(), Utc);
    }
}
