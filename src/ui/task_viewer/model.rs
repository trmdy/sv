use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::config::TasksConfig;
use crate::task::TaskRecord;

fn normalize_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_status(value: &str) -> String {
    normalize_text(value)
}

pub fn sort_tasks(tasks: &mut [TaskRecord], config: &TasksConfig, blocked_ids: &HashSet<String>) {
    crate::task::sort_tasks(tasks, config, blocked_ids);
}

pub fn nest_tasks(
    tasks: Vec<TaskRecord>,
    parent_by_child: &HashMap<String, String>,
) -> (Vec<TaskRecord>, Vec<usize>) {
    if tasks.is_empty() {
        return (tasks, Vec::new());
    }

    let base_order: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let mut order_index = HashMap::new();
    for (pos, id) in base_order.iter().enumerate() {
        order_index.insert(id.clone(), pos);
    }

    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for (child, parent) in parent_by_child {
        if order_index.contains_key(child) && order_index.contains_key(parent) {
            children_by_parent
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
        }
    }

    for children in children_by_parent.values_mut() {
        children.sort_by_key(|id| order_index.get(id).copied().unwrap_or(usize::MAX));
    }

    let mut ordered: Vec<(String, usize)> = Vec::with_capacity(base_order.len());
    let mut visited: HashSet<String> = HashSet::new();

    fn push_subtree(
        id: &str,
        depth: usize,
        children_by_parent: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        ordered: &mut Vec<(String, usize)>,
    ) {
        if !visited.insert(id.to_string()) {
            return;
        }
        ordered.push((id.to_string(), depth));
        if let Some(children) = children_by_parent.get(id) {
            for child in children {
                push_subtree(child, depth + 1, children_by_parent, visited, ordered);
            }
        }
    }

    for id in &base_order {
        let is_root = parent_by_child
            .get(id)
            .map(|parent| !order_index.contains_key(parent))
            .unwrap_or(true);
        if is_root {
            push_subtree(id, 0, &children_by_parent, &mut visited, &mut ordered);
        }
    }

    for id in &base_order {
        if !visited.contains(id) {
            push_subtree(id, 0, &children_by_parent, &mut visited, &mut ordered);
        }
    }

    let mut by_id: HashMap<String, TaskRecord> = HashMap::new();
    for task in tasks {
        by_id.insert(task.id.clone(), task);
    }

    let mut nested = Vec::with_capacity(ordered.len());
    let mut depths = Vec::with_capacity(ordered.len());
    for (id, depth) in ordered {
        if let Some(task) = by_id.remove(&id) {
            nested.push(task);
            depths.push(depth);
        }
    }

    (nested, depths)
}

pub fn group_tasks_by_epic(
    tasks: Vec<TaskRecord>,
    depths: Vec<usize>,
) -> (Vec<TaskRecord>, Vec<usize>, HashSet<String>) {
    if tasks.is_empty() {
        return (tasks, depths, HashSet::new());
    }

    let mut index_by_id = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        index_by_id.insert(task.id.clone(), idx);
    }

    let all_epic_ids: HashSet<String> = tasks.iter().filter_map(|task| task.epic.clone()).collect();
    let epic_ids: HashSet<String> = all_epic_ids
        .iter()
        .filter(|id| index_by_id.contains_key(*id))
        .cloned()
        .collect();
    if epic_ids.is_empty() {
        return (tasks, depths, epic_ids);
    }

    let mut members_by_epic: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        let Some(epic) = task.epic.as_ref() else {
            continue;
        };
        if task.id == *epic {
            continue;
        }
        members_by_epic.entry(epic.clone()).or_default().push(idx);
    }
    for members in members_by_epic.values_mut() {
        members.sort_unstable();
    }

    let mut epic_order: Vec<(usize, String)> = epic_ids
        .iter()
        .map(|epic| {
            let order_key = index_by_id
                .get(epic)
                .copied()
                .or_else(|| {
                    members_by_epic
                        .get(epic)
                        .and_then(|ids| ids.first().copied())
                })
                .unwrap_or(usize::MAX);
            (order_key, epic.clone())
        })
        .collect();
    epic_order.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let mut slots: Vec<Option<TaskRecord>> = tasks.into_iter().map(Some).collect();
    let mut grouped_tasks = Vec::with_capacity(slots.len());
    let mut grouped_depths = Vec::with_capacity(slots.len());
    let mut consumed = HashSet::new();

    for (_, epic_id) in epic_order {
        if let Some(&epic_idx) = index_by_id.get(&epic_id) {
            if consumed.insert(epic_idx) {
                if let Some(task) = slots[epic_idx].take() {
                    grouped_tasks.push(task);
                    grouped_depths.push(0);
                }
            }
        }
        if let Some(member_indices) = members_by_epic.get(&epic_id) {
            for idx in member_indices {
                if !consumed.insert(*idx) {
                    continue;
                }
                if let Some(task) = slots[*idx].take() {
                    grouped_tasks.push(task);
                    grouped_depths.push(depths.get(*idx).copied().unwrap_or(0) + 1);
                }
            }
        }
    }

    for (idx, depth) in depths.into_iter().enumerate() {
        if !consumed.insert(idx) {
            continue;
        }
        if let Some(task) = slots[idx].take() {
            grouped_tasks.push(task);
            grouped_depths.push(depth);
        }
    }

    (grouped_tasks, grouped_depths, epic_ids)
}

pub fn group_tasks_by_project(
    tasks: Vec<TaskRecord>,
    depths: Vec<usize>,
) -> (Vec<TaskRecord>, Vec<usize>, HashSet<String>) {
    if tasks.is_empty() {
        return (tasks, depths, HashSet::new());
    }

    let mut index_by_id = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        index_by_id.insert(task.id.clone(), idx);
    }

    let mut project_cache: Vec<Option<Option<String>>> = vec![None; tasks.len()];
    let mut all_project_ids: HashSet<String> = HashSet::new();
    for idx in 0..tasks.len() {
        let project = resolve_project_for_task(
            idx,
            &tasks,
            &index_by_id,
            &mut project_cache,
            &mut HashSet::new(),
        );
        if let Some(project_id) = project {
            all_project_ids.insert(project_id);
        }
    }
    let project_ids: HashSet<String> = all_project_ids;
    if project_ids.is_empty() {
        return (tasks, depths, project_ids);
    }

    let mut members_by_project: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        let Some(project) = project_cache
            .get(idx)
            .and_then(|entry| entry.as_ref())
            .and_then(|value| value.as_ref())
        else {
            continue;
        };
        if task.id == *project {
            continue;
        }
        members_by_project
            .entry(project.clone())
            .or_default()
            .push(idx);
    }
    for members in members_by_project.values_mut() {
        members.sort_unstable();
    }

    let mut project_order: Vec<(usize, String)> = project_ids
        .iter()
        .map(|project| {
            let order_key = index_by_id
                .get(project)
                .copied()
                .or_else(|| {
                    members_by_project
                        .get(project)
                        .and_then(|ids| ids.first().copied())
                })
                .unwrap_or(usize::MAX);
            (order_key, project.clone())
        })
        .collect();
    project_order.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let mut slots: Vec<Option<TaskRecord>> = tasks.into_iter().map(Some).collect();
    let mut grouped_tasks = Vec::with_capacity(slots.len());
    let mut grouped_depths = Vec::with_capacity(slots.len());
    let mut consumed = HashSet::new();

    for (_, project_id) in project_order {
        let mut has_anchor = false;
        if let Some(&project_idx) = index_by_id.get(&project_id) {
            if consumed.insert(project_idx) {
                if let Some(task) = slots[project_idx].take() {
                    grouped_tasks.push(task);
                    grouped_depths.push(0);
                    has_anchor = true;
                }
            }
        }
        if let Some(member_indices) = members_by_project.get(&project_id) {
            for idx in member_indices {
                if !consumed.insert(*idx) {
                    continue;
                }
                if let Some(task) = slots[*idx].take() {
                    grouped_tasks.push(task);
                    let extra_depth = if has_anchor { 1 } else { 0 };
                    grouped_depths.push(depths.get(*idx).copied().unwrap_or(0) + extra_depth);
                }
            }
        }
    }

    for (idx, depth) in depths.into_iter().enumerate() {
        if !consumed.insert(idx) {
            continue;
        }
        if let Some(task) = slots[idx].take() {
            grouped_tasks.push(task);
            grouped_depths.push(depth);
        }
    }

    (grouped_tasks, grouped_depths, project_ids)
}

fn resolve_project_for_task(
    idx: usize,
    tasks: &[TaskRecord],
    index_by_id: &HashMap<String, usize>,
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
        .and_then(|task| task.epic.as_ref())
        .and_then(|epic_id| index_by_id.get(epic_id))
        .copied()
    {
        resolve_project_for_task(epic_idx, tasks, index_by_id, cache, visiting)
    } else {
        None
    };

    visiting.remove(&idx);
    cache[idx] = Some(resolved.clone());
    resolved
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

#[allow(clippy::too_many_arguments)]
pub fn filter_task_indices(
    tasks: &[TaskRecord],
    query: &str,
    status_filter: Option<&str>,
    epic_filter: Option<&str>,
    project_filter: Option<&str>,
    epic_ids: &HashSet<String>,
    project_ids: &HashSet<String>,
    epics_only: bool,
    projects_only: bool,
) -> Vec<usize> {
    let query_norm = normalize_text(query);
    let status_norm = status_filter.map(normalize_status);
    let epic_norm = epic_filter.map(normalize_text);
    let project_norm = project_filter.map(normalize_text);
    let project_lookup = if project_norm.is_some() || projects_only {
        let mut index_by_id = HashMap::new();
        for (idx, task) in tasks.iter().enumerate() {
            index_by_id.insert(task.id.clone(), idx);
        }
        Some(index_by_id)
    } else {
        None
    };
    let project_ids_norm = if projects_only {
        Some(
            project_ids
                .iter()
                .map(|candidate| normalize_text(candidate))
                .collect::<HashSet<String>>(),
        )
    } else {
        None
    };
    let mut project_cache: Option<Vec<Option<Option<String>>>> =
        project_lookup.as_ref().map(|_| vec![None; tasks.len()]);
    let mut indices = Vec::new();

    for (idx, task) in tasks.iter().enumerate() {
        if epics_only && !epic_ids.contains(&task.id) {
            continue;
        }
        let mut effective_project_norm: Option<String> = None;
        if project_lookup.is_some() {
            let mut task_project = task.project.as_ref().cloned();
            if task_project.is_none() {
                if let (Some(index_by_id), Some(cache)) =
                    (project_lookup.as_ref(), project_cache.as_mut())
                {
                    task_project = resolve_project_for_task(
                        idx,
                        tasks,
                        index_by_id,
                        cache,
                        &mut HashSet::new(),
                    );
                }
            }
            effective_project_norm = task_project.as_deref().map(normalize_text);
        }

        if projects_only {
            let is_project_anchor = project_ids.contains(&task.id);
            let has_project_membership = effective_project_norm
                .as_deref()
                .and_then(|project| project_ids_norm.as_ref().map(|set| set.contains(project)))
                .unwrap_or(false);
            if !is_project_anchor && !has_project_membership {
                continue;
            }
        }

        if let Some(status) = status_norm.as_deref() {
            if normalize_status(&task.status) != status {
                continue;
            }
        }

        if let Some(epic) = epic_norm.as_deref() {
            let task_id = normalize_text(&task.id);
            let task_epic = task.epic.as_deref().map(normalize_text);
            if task_id != epic && task_epic.as_deref() != Some(epic) {
                continue;
            }
        }
        if let Some(project) = project_norm.as_deref() {
            let task_id = normalize_text(&task.id);
            if task_id != project && effective_project_norm.as_deref() != Some(project) {
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
        if let Some(index) = tasks
            .iter()
            .position(|task| normalize_text(&task.id) == normalized)
        {
            if filtered.contains(&index) {
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
            epic: None,
            project: None,
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
        let indices = filter_task_indices(
            &tasks,
            "SYNC",
            None,
            None,
            None,
            &HashSet::new(),
            &HashSet::new(),
            false,
            false,
        );
        assert_eq!(indices, vec![0]);

        let indices = filter_task_indices(
            &tasks,
            "watch",
            None,
            None,
            None,
            &HashSet::new(),
            &HashSet::new(),
            false,
            false,
        );
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn filter_combines_status_and_text() {
        let now = Utc::now();
        let tasks = vec![
            task("sv-aaa", "Fix Sync", "open", "P2", now),
            task("sv-bbb", "Fix Sync", "closed", "P2", now),
        ];
        let indices = filter_task_indices(
            &tasks,
            "sync",
            Some("open"),
            None,
            None,
            &HashSet::new(),
            &HashSet::new(),
            false,
            false,
        );
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn filter_supports_epic_and_epics_only_mode() {
        let now = Utc::now();
        let mut epic = task("sv-epic", "Epic", "open", "P1", now);
        epic.epic = None;
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.epic = Some("sv-epic".to_string());
        let tasks = vec![epic, child];
        let mut epic_ids = HashSet::new();
        epic_ids.insert("sv-epic".to_string());

        let filtered = filter_task_indices(
            &tasks,
            "",
            None,
            Some("sv-epic"),
            None,
            &epic_ids,
            &HashSet::new(),
            false,
            false,
        );
        assert_eq!(filtered, vec![0, 1]);

        let epic_only = filter_task_indices(
            &tasks,
            "",
            None,
            None,
            None,
            &epic_ids,
            &HashSet::new(),
            true,
            false,
        );
        assert_eq!(epic_only, vec![0]);
    }

    #[test]
    fn filter_supports_project_and_projects_only_mode() {
        let now = Utc::now();
        let mut project = task("sv-project", "Project", "open", "P1", now);
        project.project = None;
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.project = Some("sv-project".to_string());
        let tasks = vec![project, child];
        let mut project_ids = HashSet::new();
        project_ids.insert("sv-project".to_string());

        let filtered = filter_task_indices(
            &tasks,
            "",
            None,
            None,
            Some("sv-project"),
            &HashSet::new(),
            &project_ids,
            false,
            false,
        );
        assert_eq!(filtered, vec![0, 1]);

        let project_only = filter_task_indices(
            &tasks,
            "",
            None,
            None,
            None,
            &HashSet::new(),
            &project_ids,
            false,
            true,
        );
        assert_eq!(project_only, vec![0, 1]);
    }

    #[test]
    fn filter_supports_project_inherited_from_epic() {
        let now = Utc::now();
        let mut project = task("sv-project", "Project", "open", "P1", now);
        project.project = None;
        let mut epic = task("sv-epic", "Epic", "open", "P1", now);
        epic.project = Some("sv-project".to_string());
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.epic = Some("sv-epic".to_string());
        let tasks = vec![project, epic, child];
        let mut project_ids = HashSet::new();
        project_ids.insert("sv-project".to_string());

        let filtered = filter_task_indices(
            &tasks,
            "",
            None,
            None,
            Some("sv-project"),
            &HashSet::new(),
            &project_ids,
            false,
            false,
        );
        assert_eq!(filtered, vec![0, 1, 2]);
    }

    #[test]
    fn projects_only_mode_includes_non_anchor_project_members() {
        let now = Utc::now();
        let mut epic = task("sv-epic", "Epic", "open", "P1", now);
        epic.project = Some("prj-alpha".to_string());
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.epic = Some("sv-epic".to_string());
        let unrelated = task("sv-free", "Free", "open", "P2", now);
        let tasks = vec![epic, child, unrelated];

        let mut project_ids = HashSet::new();
        project_ids.insert("prj-alpha".to_string());
        let filtered = filter_task_indices(
            &tasks,
            "",
            None,
            None,
            None,
            &HashSet::new(),
            &project_ids,
            false,
            true,
        );
        assert_eq!(filtered, vec![0, 1]);
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
            task(
                "sv-3",
                "Third",
                "open",
                "P0",
                now + chrono::Duration::seconds(10),
            ),
            task("sv-2", "Second", "open", "P0", now),
        ];
        sort_tasks(&mut tasks, &config, &blocked_ids);
        assert_eq!(tasks[0].id, "sv-2");
        assert_eq!(tasks[1].id, "sv-3");
        assert_eq!(tasks[2].id, "sv-1");
        assert_eq!(tasks[3].id, "sv-4");
    }

    #[test]
    fn nest_tasks_groups_children_under_parents() {
        let now = Utc::now();
        let tasks = vec![
            task("sv-parent", "Parent", "open", "P1", now),
            task("sv-root", "Root", "open", "P1", now),
            task("sv-child", "Child", "open", "P1", now),
            task("sv-grand", "Grand", "open", "P1", now),
        ];
        let mut parent_by_child = HashMap::new();
        parent_by_child.insert("sv-child".to_string(), "sv-parent".to_string());
        parent_by_child.insert("sv-grand".to_string(), "sv-child".to_string());

        let (nested, depths) = nest_tasks(tasks, &parent_by_child);
        let ids: Vec<&str> = nested.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, vec!["sv-parent", "sv-child", "sv-grand", "sv-root"]);
        assert_eq!(depths, vec![0, 1, 2, 0]);
    }

    #[test]
    fn group_tasks_by_epic_places_members_under_epic() {
        let now = Utc::now();
        let mut epic = task("sv-epic", "Epic", "open", "P1", now);
        epic.epic = None;
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.epic = Some("sv-epic".to_string());
        let root = task("sv-root", "Root", "open", "P2", now);

        let (grouped, depths, epic_ids) =
            group_tasks_by_epic(vec![root, child, epic], vec![0, 0, 0]);
        let ids: Vec<&str> = grouped.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, vec!["sv-epic", "sv-child", "sv-root"]);
        assert_eq!(depths, vec![0, 1, 0]);
        assert!(epic_ids.contains("sv-epic"));
    }

    #[test]
    fn group_tasks_by_project_places_members_under_project() {
        let now = Utc::now();
        let mut project = task("sv-project", "Project", "open", "P1", now);
        project.project = None;
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.project = Some("sv-project".to_string());
        let root = task("sv-root", "Root", "open", "P2", now);

        let (grouped, depths, project_ids) =
            group_tasks_by_project(vec![root, child, project], vec![0, 0, 0]);
        let ids: Vec<&str> = grouped.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, vec!["sv-project", "sv-child", "sv-root"]);
        assert_eq!(depths, vec![0, 1, 0]);
        assert!(project_ids.contains("sv-project"));
    }

    #[test]
    fn group_tasks_by_project_includes_epic_descendants_without_direct_project() {
        let now = Utc::now();
        let mut project = task("sv-project", "Project", "open", "P1", now);
        project.project = None;

        let mut epic_a = task("sv-epic-a", "Epic A", "open", "P1", now);
        epic_a.project = Some("sv-project".to_string());
        let mut epic_b = task("sv-epic-b", "Epic B", "open", "P1", now);
        epic_b.project = Some("sv-project".to_string());

        let mut task_a = task("sv-task-a", "Task A", "open", "P2", now);
        task_a.epic = Some("sv-epic-a".to_string());
        let mut task_b = task("sv-task-b", "Task B", "open", "P2", now);
        task_b.epic = Some("sv-epic-b".to_string());

        let (epic_grouped, epic_depths, _) = group_tasks_by_epic(
            vec![project, epic_a, epic_b, task_a, task_b],
            vec![0, 0, 0, 0, 0],
        );
        let (grouped, depths, project_ids) = group_tasks_by_project(epic_grouped, epic_depths);

        let ids: Vec<&str> = grouped.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "sv-project",
                "sv-epic-a",
                "sv-task-a",
                "sv-epic-b",
                "sv-task-b"
            ]
        );
        assert_eq!(depths, vec![0, 1, 2, 1, 2]);
        assert!(project_ids.contains("sv-project"));
    }

    #[test]
    fn group_tasks_by_project_without_anchor_keeps_root_depth() {
        let now = Utc::now();
        let mut epic = task("sv-epic", "Epic", "open", "P1", now);
        epic.project = Some("prj-alpha".to_string());
        let mut child = task("sv-child", "Child", "open", "P2", now);
        child.epic = Some("sv-epic".to_string());

        let (grouped, depths, project_ids) = group_tasks_by_project(vec![epic, child], vec![0, 1]);
        let ids: Vec<&str> = grouped.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, vec!["sv-epic", "sv-child"]);
        assert_eq!(depths, vec![0, 1]);
        assert!(project_ids.contains("prj-alpha"));
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
