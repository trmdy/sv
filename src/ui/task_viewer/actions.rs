use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::task::{TaskEvent, TaskEventType, TaskRecord, TaskStore};

#[derive(Debug, Clone)]
pub struct NewTaskInput {
    pub title: String,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct EditTaskInput {
    pub title: String,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub changed: bool,
    pub message: String,
    pub task_id: Option<String>,
}

pub fn create_task(
    store: &TaskStore,
    actor: Option<String>,
    input: NewTaskInput,
) -> Result<ActionOutcome> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidArgument("title cannot be empty".to_string()));
    }

    let parent = input
        .parent
        .as_deref()
        .and_then(non_empty)
        .map(|value| store.resolve_task_id(&value))
        .transpose()?;
    if let Some(parent_id) = parent.as_deref() {
        ensure_parent_accepts_children(store, parent_id)?;
    }

    let children = resolve_children(store, &input.children)?;
    let blocks = resolve_task_list(store, &input.blocks)?;
    let blocked_by = resolve_task_list(store, &input.blocked_by)?;

    let normalized_priority = input
        .priority
        .as_deref()
        .and_then(non_empty)
        .map(|value| store.normalize_priority(&value))
        .transpose()?;
    let priority = normalized_priority.unwrap_or_else(|| store.default_priority());
    let status = store.config().default_status.clone();
    let body = normalize_body(&input.body);

    let task_id = store.generate_task_id()?;
    let mut event = TaskEvent::new(TaskEventType::TaskCreated, task_id.clone());
    event.actor = actor.clone();
    event.title = Some(title.to_string());
    event.body = body;
    event.status = Some(status);
    event.priority = Some(priority.clone());
    store.append_event(event)?;

    if let Some(parent_id) = parent {
        let mut parent_event = TaskEvent::new(TaskEventType::TaskParentSet, task_id.clone());
        parent_event.actor = actor.clone();
        parent_event.related_task_id = Some(parent_id);
        store.append_event(parent_event)?;
    }

    for child in children {
        if child == task_id {
            return Err(Error::InvalidArgument(
                "child cannot match parent".to_string(),
            ));
        }
        let relations = store.relations(&child)?;
        if relations.parent.as_deref() == Some(task_id.as_str()) {
            continue;
        }
        let mut parent_event = TaskEvent::new(TaskEventType::TaskParentSet, child.clone());
        parent_event.actor = actor.clone();
        parent_event.related_task_id = Some(task_id.clone());
        store.append_event(parent_event)?;
    }

    for blocked in blocks {
        if blocked == task_id {
            return Err(Error::InvalidArgument(
                "task cannot block itself".to_string(),
            ));
        }
        let mut event = TaskEvent::new(TaskEventType::TaskBlocked, task_id.clone());
        event.actor = actor.clone();
        event.related_task_id = Some(blocked);
        store.append_event(event)?;
    }

    for blocker in blocked_by {
        if blocker == task_id {
            return Err(Error::InvalidArgument(
                "task cannot be blocked by itself".to_string(),
            ));
        }
        let mut event = TaskEvent::new(TaskEventType::TaskBlocked, blocker);
        event.actor = actor.clone();
        event.related_task_id = Some(task_id.clone());
        store.append_event(event)?;
    }

    Ok(ActionOutcome {
        changed: true,
        message: format!("created {task_id}"),
        task_id: Some(task_id),
    })
}

pub fn edit_task(
    store: &TaskStore,
    actor: Option<String>,
    task_id: &str,
    input: EditTaskInput,
) -> Result<ActionOutcome> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidArgument("title cannot be empty".to_string()));
    }
    let task = load_task(store, task_id)?;
    let normalized_body = normalize_body(&input.body);
    let title_changed = title != task.title;
    let body_changed = normalized_body != task.body;

    let normalized_priority = input
        .priority
        .as_deref()
        .and_then(non_empty)
        .map(|value| store.normalize_priority(&value))
        .transpose()?;
    let priority_changed = normalized_priority
        .as_ref()
        .map(|value| value != &task.priority)
        .unwrap_or(false);

    let relations = store.relations(task_id)?;
    let parent_input = input
        .parent
        .as_deref()
        .and_then(non_empty)
        .map(|value| store.resolve_task_id(&value))
        .transpose()?;
    let parent_changed = match (relations.parent.as_deref(), parent_input.as_deref()) {
        (None, None) => false,
        (Some(existing), Some(next)) => existing != next,
        (Some(_), None) => true,
        (None, Some(_)) => true,
    };

    let children = resolve_children(store, &input.children)?;
    let current_children: HashSet<String> = relations.children.iter().cloned().collect();
    let desired_children: HashSet<String> = children.iter().cloned().collect();
    let mut children_to_add: Vec<String> = desired_children
        .difference(&current_children)
        .cloned()
        .collect();
    let mut children_to_remove: Vec<String> = current_children
        .difference(&desired_children)
        .cloned()
        .collect();
    children_to_add.sort();
    children_to_remove.sort();
    let children_changed = !(children_to_add.is_empty() && children_to_remove.is_empty());

    let blocks = resolve_task_list(store, &input.blocks)?;
    if blocks.iter().any(|id| id == task_id) {
        return Err(Error::InvalidArgument(
            "task cannot block itself".to_string(),
        ));
    }
    let blocked_by = resolve_task_list(store, &input.blocked_by)?;
    if blocked_by.iter().any(|id| id == task_id) {
        return Err(Error::InvalidArgument(
            "task cannot be blocked by itself".to_string(),
        ));
    }

    let (blocks_to_add, blocks_to_remove) = diff_relation_sets(&relations.blocks, &blocks);
    let (blocked_by_to_add, blocked_by_to_remove) =
        diff_relation_sets(&relations.blocked_by, &blocked_by);
    let blocks_changed = !(blocks_to_add.is_empty() && blocks_to_remove.is_empty());
    let blocked_by_changed = !(blocked_by_to_add.is_empty() && blocked_by_to_remove.is_empty());

    if !title_changed
        && !body_changed
        && !priority_changed
        && !parent_changed
        && !children_changed
        && !blocks_changed
        && !blocked_by_changed
    {
        return Ok(ActionOutcome {
            changed: false,
            message: "no changes".to_string(),
            task_id: Some(task_id.to_string()),
        });
    }

    if title_changed || body_changed {
        let mut event = TaskEvent::new(TaskEventType::TaskEdited, task_id.to_string());
        event.actor = actor.clone();
        if title_changed {
            event.title = Some(title.to_string());
        }
        if body_changed {
            event.body = Some(input.body);
        }
        store.append_event(event)?;
    }

    if let Some(priority) = normalized_priority {
        if priority_changed {
            let mut event = TaskEvent::new(TaskEventType::TaskPriorityChanged, task_id.to_string());
            event.actor = actor.clone();
            event.priority = Some(priority);
            store.append_event(event)?;
        }
    }

    if parent_changed {
        match parent_input {
            Some(parent) => {
                if parent == task_id {
                    return Err(Error::InvalidArgument(
                        "parent cannot match child".to_string(),
                    ));
                }
                ensure_parent_accepts_children(store, &parent)?;
                let mut event = TaskEvent::new(TaskEventType::TaskParentSet, task_id.to_string());
                event.actor = actor.clone();
                event.related_task_id = Some(parent);
                store.append_event(event)?;
            }
            None => {
                if let Some(existing) = relations.parent.as_deref() {
                    let mut event =
                        TaskEvent::new(TaskEventType::TaskParentCleared, task_id.to_string());
                    event.actor = actor.clone();
                    event.related_task_id = Some(existing.to_string());
                    store.append_event(event)?;
                }
            }
        }
    }

    if !children_to_add.is_empty() {
        ensure_parent_accepts_children(store, task_id)?;
    }
    for child in children_to_add {
        if child == task_id {
            return Err(Error::InvalidArgument(
                "child cannot match parent".to_string(),
            ));
        }
        let child_relations = store.relations(&child)?;
        if child_relations.parent.as_deref() == Some(task_id) {
            continue;
        }
        let mut parent_event = TaskEvent::new(TaskEventType::TaskParentSet, child.clone());
        parent_event.actor = actor.clone();
        parent_event.related_task_id = Some(task_id.to_string());
        store.append_event(parent_event)?;
    }

    for child in children_to_remove {
        let child_relations = store.relations(&child)?;
        if child_relations.parent.as_deref() != Some(task_id) {
            continue;
        }
        let mut event = TaskEvent::new(TaskEventType::TaskParentCleared, child.clone());
        event.actor = actor.clone();
        event.related_task_id = Some(task_id.to_string());
        store.append_event(event)?;
    }

    for blocked in blocks_to_add {
        let mut event = TaskEvent::new(TaskEventType::TaskBlocked, task_id.to_string());
        event.actor = actor.clone();
        event.related_task_id = Some(blocked);
        store.append_event(event)?;
    }

    for blocked in blocks_to_remove {
        let mut event = TaskEvent::new(TaskEventType::TaskUnblocked, task_id.to_string());
        event.actor = actor.clone();
        event.related_task_id = Some(blocked);
        store.append_event(event)?;
    }

    for blocker in blocked_by_to_add {
        let mut event = TaskEvent::new(TaskEventType::TaskBlocked, blocker);
        event.actor = actor.clone();
        event.related_task_id = Some(task_id.to_string());
        store.append_event(event)?;
    }

    for blocker in blocked_by_to_remove {
        let mut event = TaskEvent::new(TaskEventType::TaskUnblocked, blocker);
        event.actor = actor.clone();
        event.related_task_id = Some(task_id.to_string());
        store.append_event(event)?;
    }

    Ok(ActionOutcome {
        changed: true,
        message: "task updated".to_string(),
        task_id: Some(task_id.to_string()),
    })
}

pub fn set_blocked_by(
    store: &TaskStore,
    actor: Option<String>,
    task_id: &str,
    blocked_by: Vec<String>,
) -> Result<ActionOutcome> {
    let relations = store.relations(task_id)?;
    let resolved = resolve_task_list(store, &blocked_by)?;
    if resolved.iter().any(|id| id == task_id) {
        return Err(Error::InvalidArgument(
            "task cannot be blocked by itself".to_string(),
        ));
    }
    let (to_add, to_remove) = diff_relation_sets(&relations.blocked_by, &resolved);
    if to_add.is_empty() && to_remove.is_empty() {
        return Ok(ActionOutcome {
            changed: false,
            message: "no changes".to_string(),
            task_id: Some(task_id.to_string()),
        });
    }

    for blocker in to_add {
        let mut event = TaskEvent::new(TaskEventType::TaskBlocked, blocker);
        event.actor = actor.clone();
        event.related_task_id = Some(task_id.to_string());
        store.append_event(event)?;
    }

    for blocker in to_remove {
        let mut event = TaskEvent::new(TaskEventType::TaskUnblocked, blocker);
        event.actor = actor.clone();
        event.related_task_id = Some(task_id.to_string());
        store.append_event(event)?;
    }

    Ok(ActionOutcome {
        changed: true,
        message: "blocked by updated".to_string(),
        task_id: Some(task_id.to_string()),
    })
}

pub fn delete_task(
    store: &TaskStore,
    actor: Option<String>,
    task_id: &str,
) -> Result<ActionOutcome> {
    let resolved = store.resolve_task_id(task_id)?;
    let details = store.details(&resolved)?;
    let relations = details.relations;

    if let Some(parent) = relations.parent {
        let mut event = TaskEvent::new(TaskEventType::TaskParentCleared, resolved.clone());
        event.actor = actor.clone();
        event.related_task_id = Some(parent);
        store.append_event(event)?;
    }

    for child in relations.children {
        let mut event = TaskEvent::new(TaskEventType::TaskParentCleared, child);
        event.actor = actor.clone();
        event.related_task_id = Some(resolved.clone());
        store.append_event(event)?;
    }

    for blocked in relations.blocks {
        let mut event = TaskEvent::new(TaskEventType::TaskUnblocked, resolved.clone());
        event.actor = actor.clone();
        event.related_task_id = Some(blocked);
        store.append_event(event)?;
    }

    for blocker in relations.blocked_by {
        let mut event = TaskEvent::new(TaskEventType::TaskUnblocked, blocker);
        event.actor = actor.clone();
        event.related_task_id = Some(resolved.clone());
        store.append_event(event)?;
    }

    for related in relations.relates {
        let mut event = TaskEvent::new(TaskEventType::TaskUnrelated, resolved.clone());
        event.actor = actor.clone();
        event.related_task_id = Some(related.id);
        store.append_event(event)?;
    }

    let mut event = TaskEvent::new(TaskEventType::TaskDeleted, resolved.clone());
    event.actor = actor.clone();
    store.append_event(event)?;

    Ok(ActionOutcome {
        changed: true,
        message: format!("deleted {resolved}"),
        task_id: Some(resolved),
    })
}

pub fn change_priority(
    store: &TaskStore,
    actor: Option<String>,
    task_id: &str,
    priority: &str,
) -> Result<ActionOutcome> {
    let normalized = store.normalize_priority(priority)?;
    let task = load_task(store, task_id)?;
    if task.priority.eq_ignore_ascii_case(&normalized) {
        return Ok(ActionOutcome {
            changed: false,
            message: "priority unchanged".to_string(),
            task_id: Some(task_id.to_string()),
        });
    }

    let mut event = TaskEvent::new(TaskEventType::TaskPriorityChanged, task_id.to_string());
    event.actor = actor.clone();
    event.priority = Some(normalized.clone());
    store.append_event(event)?;

    Ok(ActionOutcome {
        changed: true,
        message: format!("priority set to {normalized}"),
        task_id: Some(task_id.to_string()),
    })
}

pub fn change_status(
    store: &TaskStore,
    actor: Option<String>,
    task_id: &str,
    status: &str,
) -> Result<ActionOutcome> {
    store.validate_status(status)?;
    if is_closed_status(store, status) && is_project_grouping(store, task_id)? {
        return Err(Error::InvalidArgument(
            "project groups cannot be completed; close member tasks instead".to_string(),
        ));
    }
    let task = load_task(store, task_id)?;
    if task.status.eq_ignore_ascii_case(status) {
        return Ok(ActionOutcome {
            changed: false,
            message: "status unchanged".to_string(),
            task_id: Some(task_id.to_string()),
        });
    }

    let mut event = TaskEvent::new(TaskEventType::TaskStatusChanged, task_id.to_string());
    event.actor = actor.clone();
    event.status = Some(status.to_string());
    store.append_event(event)?;

    Ok(ActionOutcome {
        changed: true,
        message: format!("status set to {status}"),
        task_id: Some(task_id.to_string()),
    })
}

fn load_task(store: &TaskStore, task_id: &str) -> Result<TaskRecord> {
    let tasks = store.list(None)?;
    tasks
        .into_iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| Error::InvalidArgument(format!("task not found: {task_id}")))
}

fn normalize_body(body: &str) -> Option<String> {
    if body.trim().is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn resolve_children(store: &TaskStore, children: &[String]) -> Result<Vec<String>> {
    resolve_task_list(store, children)
}

fn resolve_task_list(store: &TaskStore, values: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resolved = store.resolve_task_id(trimmed)?;
        if !out.iter().any(|existing| existing == &resolved) {
            out.push(resolved);
        }
    }
    Ok(out)
}

fn is_closed_status(store: &TaskStore, status: &str) -> bool {
    store
        .config()
        .closed_statuses
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(status.trim()))
}

fn is_project_grouping(store: &TaskStore, task_id: &str) -> Result<bool> {
    let relations = store.relations(task_id)?;
    Ok(!relations.project_tasks.is_empty())
}

fn ensure_parent_accepts_children(store: &TaskStore, parent_id: &str) -> Result<()> {
    let relations = store.relations(parent_id)?;
    if relations.project_tasks.is_empty() {
        return Ok(());
    }
    Err(Error::InvalidArgument(
        "tasks cannot be children of project groups".to_string(),
    ))
}

fn diff_relation_sets(current: &[String], desired: &[String]) -> (Vec<String>, Vec<String>) {
    let current_set: HashSet<String> = current.iter().cloned().collect();
    let desired_set: HashSet<String> = desired.iter().cloned().collect();
    let mut to_add: Vec<String> = desired_set.difference(&current_set).cloned().collect();
    let mut to_remove: Vec<String> = current_set.difference(&desired_set).cloned().collect();
    to_add.sort();
    to_remove.sort();
    (to_add, to_remove)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::config::TasksConfig;
    use crate::storage::Storage;
    use tempfile::TempDir;

    fn setup_store() -> (TempDir, TaskStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".git")).expect("git dir");
        let root = dir.path().to_path_buf();
        let storage = Storage::new(root.clone(), root.join(".git"), root.clone());
        let store = TaskStore::new(storage, TasksConfig::default());
        (dir, store)
    }

    fn seed_task(store: &TaskStore, title: &str) -> String {
        let task_id = store.generate_task_id().expect("id");
        let mut event = TaskEvent::new(TaskEventType::TaskCreated, task_id.clone());
        event.title = Some(title.to_string());
        event.status = Some(store.config().default_status.clone());
        event.priority = Some(store.default_priority());
        store.append_event(event).expect("append");
        task_id
    }

    #[test]
    fn new_task_rejects_empty_title() {
        let (_dir, store) = setup_store();
        let err = create_task(
            &store,
            None,
            NewTaskInput {
                title: "   ".to_string(),
                priority: None,
                parent: None,
                children: Vec::new(),
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                body: "".to_string(),
            },
        )
        .expect_err("should reject");
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn edit_task_noop_when_unchanged() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Keep");
        let outcome = edit_task(
            &store,
            None,
            &task_id,
            EditTaskInput {
                title: "Keep".to_string(),
                priority: None,
                parent: None,
                children: Vec::new(),
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                body: "".to_string(),
            },
        )
        .expect("edit");
        assert!(!outcome.changed);
    }

    #[test]
    fn edit_task_updates_title_and_body() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Old");
        let outcome = edit_task(
            &store,
            None,
            &task_id,
            EditTaskInput {
                title: "New".to_string(),
                priority: None,
                parent: None,
                children: Vec::new(),
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                body: "Details".to_string(),
            },
        )
        .expect("edit");
        assert!(outcome.changed);

        let updated = load_task(&store, &task_id).expect("load");
        assert_eq!(updated.title, "New");
        assert_eq!(updated.body.as_deref(), Some("Details"));
    }

    #[test]
    fn edit_task_updates_block_relations() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Target");
        let blocked_id = seed_task(&store, "Blocked");
        let blocker_id = seed_task(&store, "Blocker");

        let outcome = edit_task(
            &store,
            None,
            &task_id,
            EditTaskInput {
                title: "Target".to_string(),
                priority: None,
                parent: None,
                children: Vec::new(),
                blocks: vec![blocked_id.clone()],
                blocked_by: vec![blocker_id.clone()],
                body: "".to_string(),
            },
        )
        .expect("edit");
        assert!(outcome.changed);

        let relations = store.relations(&task_id).expect("relations");
        assert_eq!(relations.blocks, vec![blocked_id]);
        assert_eq!(relations.blocked_by, vec![blocker_id]);
    }

    #[test]
    fn set_blocked_by_updates_relations() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Target");
        let blocker_id = seed_task(&store, "Blocker");

        let outcome =
            set_blocked_by(&store, None, &task_id, vec![blocker_id.clone()]).expect("blocked by");
        assert!(outcome.changed);

        let relations = store.relations(&task_id).expect("relations");
        assert_eq!(relations.blocked_by, vec![blocker_id]);
    }

    #[test]
    fn edit_task_clears_children_when_empty() {
        let (_dir, store) = setup_store();
        let parent_id = seed_task(&store, "Parent");
        let child_id = seed_task(&store, "Child");
        let mut event = TaskEvent::new(TaskEventType::TaskParentSet, child_id.clone());
        event.related_task_id = Some(parent_id.clone());
        store.append_event(event).expect("append");

        let outcome = edit_task(
            &store,
            None,
            &parent_id,
            EditTaskInput {
                title: "Parent".to_string(),
                priority: None,
                parent: None,
                children: Vec::new(),
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                body: "".to_string(),
            },
        )
        .expect("edit");
        assert!(outcome.changed);

        let relations = store.relations(&parent_id).expect("relations");
        assert!(relations.children.is_empty());
        let child_relations = store.relations(&child_id).expect("child relations");
        assert!(child_relations.parent.is_none());
    }

    #[test]
    fn delete_task_clears_parent_relation() {
        let (_dir, store) = setup_store();
        let parent_id = seed_task(&store, "Parent");
        let child_id = seed_task(&store, "Child");
        let mut event = TaskEvent::new(TaskEventType::TaskParentSet, child_id.clone());
        event.related_task_id = Some(parent_id.clone());
        store.append_event(event).expect("append");

        let outcome = delete_task(&store, None, &parent_id).expect("delete");
        assert!(outcome.changed);

        let tasks = store.list(None).expect("list");
        assert!(!tasks.iter().any(|task| task.id == parent_id));
        let child_relations = store.relations(&child_id).expect("child relations");
        assert!(child_relations.parent.is_none());
    }

    #[test]
    fn priority_change_noop_when_same() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Keep");
        let outcome = change_priority(&store, None, &task_id, "P2").expect("priority");
        assert!(!outcome.changed);
    }

    #[test]
    fn priority_change_updates_record() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Keep");
        let outcome = change_priority(&store, None, &task_id, "P0").expect("priority");
        assert!(outcome.changed);
        let updated = load_task(&store, &task_id).expect("load");
        assert_eq!(updated.priority, "P0");
    }

    #[test]
    fn status_change_updates_record() {
        let (_dir, store) = setup_store();
        let task_id = seed_task(&store, "Keep");
        let outcome = change_status(&store, None, &task_id, "in_progress").expect("status");
        assert!(outcome.changed);
        let updated = load_task(&store, &task_id).expect("load");
        assert_eq!(updated.status, "in_progress");
    }

    #[test]
    fn status_change_rejects_closed_status_for_project_grouping() {
        let (_dir, store) = setup_store();
        let project_id = seed_task(&store, "Project");
        let child_id = seed_task(&store, "Child");
        let mut event = TaskEvent::new(TaskEventType::TaskProjectSet, child_id);
        event.related_task_id = Some(project_id.clone());
        store.append_event(event).expect("project set");

        let err = change_status(&store, None, &project_id, "closed").expect_err("status");
        assert!(matches!(err, Error::InvalidArgument(_)));
        assert!(err
            .to_string()
            .contains("project groups cannot be completed"));
    }

    #[test]
    fn edit_task_rejects_project_group_parent() {
        let (_dir, store) = setup_store();
        let project_id = seed_task(&store, "Project");
        let member_id = seed_task(&store, "Member");
        let child_id = seed_task(&store, "Child");

        let mut set_project = TaskEvent::new(TaskEventType::TaskProjectSet, member_id);
        set_project.related_task_id = Some(project_id.clone());
        store.append_event(set_project).expect("project set");

        let err = edit_task(
            &store,
            None,
            &child_id,
            EditTaskInput {
                title: "Child".to_string(),
                priority: None,
                parent: Some(project_id),
                children: Vec::new(),
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                body: "".to_string(),
            },
        )
        .expect_err("edit should fail");

        assert!(matches!(err, Error::InvalidArgument(_)));
        assert!(err
            .to_string()
            .contains("tasks cannot be children of project groups"));
    }

    #[test]
    fn edit_task_rejects_children_for_project_group() {
        let (_dir, store) = setup_store();
        let project_id = seed_task(&store, "Project");
        let member_id = seed_task(&store, "Member");
        let child_id = seed_task(&store, "Child");

        let mut set_project = TaskEvent::new(TaskEventType::TaskProjectSet, member_id);
        set_project.related_task_id = Some(project_id.clone());
        store.append_event(set_project).expect("project set");

        let err = edit_task(
            &store,
            None,
            &project_id,
            EditTaskInput {
                title: "Project".to_string(),
                priority: None,
                parent: None,
                children: vec![child_id],
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                body: "".to_string(),
            },
        )
        .expect_err("edit should fail");

        assert!(matches!(err, Error::InvalidArgument(_)));
        assert!(err
            .to_string()
            .contains("tasks cannot be children of project groups"));
    }
}
