mod support;

use sv::config::Config;
use sv::storage::Storage;
use sv::task::{TaskEvent, TaskEventType, TaskStore};

use support::{sv_cmd, TestRepo};

#[test]
fn status_runs_in_repo() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("status")
        .assert()
        .success();

    Ok(())
}

#[test]
fn status_runs_with_relation_events() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    let repo_root = repo.path().to_path_buf();
    let storage = Storage::new(repo_root.clone(), repo_root.join(".git"), repo_root.clone());
    let config = Config::load_from_repo(&repo_root);
    let store = TaskStore::new(storage, config.tasks);

    let mut task_a = TaskEvent::new(TaskEventType::TaskCreated, "task-a");
    task_a.title = Some("Task A".to_string());
    store.append_event(task_a)?;

    let mut task_b = TaskEvent::new(TaskEventType::TaskCreated, "task-b");
    task_b.title = Some("Task B".to_string());
    store.append_event(task_b)?;

    let mut relate = TaskEvent::new(TaskEventType::TaskRelated, "task-a");
    relate.related_task_id = Some("task-b".to_string());
    relate.relation_description = Some("shares context".to_string());
    store.append_event(relate)?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("status")
        .assert()
        .success();

    Ok(())
}
