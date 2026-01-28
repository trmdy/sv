use crate::error::{Error, Result};
use crate::task::{TaskEvent, TaskEventType, TaskRecord, TaskStore};

#[derive(Debug, Clone)]
pub struct RelateInput {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct NewTaskInput {
    pub title: String,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub relates: Option<RelateInput>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct EditTaskInput {
    pub title: String,
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
        .and_then(|value| non_empty(value))
        .map(|value| store.resolve_task_id(&value))
        .transpose()?;

    let relates = match input.relates {
        Some(relate) => {
            let id = relate.id.trim();
            let description = relate.description.trim();
            if id.is_empty() || description.is_empty() {
                return Err(Error::InvalidArgument(
                    "relation id and description required".to_string(),
                ));
            }
            let resolved = store.resolve_task_id(id)?;
            Some(RelateInput {
                id: resolved,
                description: description.to_string(),
            })
        }
        None => None,
    };

    let normalized_priority = input
        .priority
        .as_deref()
        .and_then(|value| non_empty(value))
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

    if let Some(relate) = relates {
        let mut relate_event = TaskEvent::new(TaskEventType::TaskRelated, task_id.clone());
        relate_event.actor = actor.clone();
        relate_event.related_task_id = Some(relate.id);
        relate_event.relation_description = Some(relate.description);
        store.append_event(relate_event)?;
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

    if !title_changed && !body_changed {
        return Ok(ActionOutcome {
            changed: false,
            message: "no changes".to_string(),
            task_id: Some(task_id.to_string()),
        });
    }

    let mut event = TaskEvent::new(TaskEventType::TaskEdited, task_id.to_string());
    event.actor = actor.clone();
    if title_changed {
        event.title = Some(title.to_string());
    }
    if body_changed {
        event.body = Some(input.body);
    }
    store.append_event(event)?;

    Ok(ActionOutcome {
        changed: true,
        message: "task updated".to_string(),
        task_id: Some(task_id.to_string()),
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
                relates: None,
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
}
