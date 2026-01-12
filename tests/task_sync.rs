use sv::config::Config;
use sv::storage::Storage;
use sv::task::{TaskEvent, TaskEventType, TaskStore};

mod support;

#[test]
fn sync_merges_tracked_and_shared_logs() {
    let repo = support::TestRepo::init().expect("repo");
    repo.init_sv_dirs().expect("sv dirs");

    let repo_root = repo.path().to_path_buf();
    let storage = Storage::new(repo_root.clone(), repo_root.join(".git"), repo_root.clone());
    let config = Config::load_from_repo(&repo_root);
    let store = TaskStore::new(storage.clone(), config.tasks);

    let mut event1 = TaskEvent::new(TaskEventType::TaskCreated, "task-1");
    event1.title = Some("One".to_string());
    storage
        .append_jsonl(&store.tracked_log_path(), &event1)
        .expect("append tracked");

    let mut event2 = TaskEvent::new(TaskEventType::TaskCreated, "task-2");
    event2.title = Some("Two".to_string());
    storage
        .append_jsonl(&store.shared_log_path(), &event2)
        .expect("append shared");

    let report = store.sync(None).expect("sync");
    assert_eq!(report.total_events, 2);

    let merged: Vec<TaskEvent> = storage
        .read_jsonl(&store.tracked_log_path())
        .expect("read merged");
    assert_eq!(merged.len(), 2);
}
