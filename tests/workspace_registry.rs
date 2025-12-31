use tempfile::TempDir;

use sv::storage::{Storage, WorkspaceEntry};

fn setup_storage() -> (TempDir, Storage) {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir(repo_root.join(".git")).unwrap();
    let storage = Storage::for_repo(repo_root);
    (temp, storage)
}

#[test]
fn add_workspace_requires_existing_path() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();
    let entry = WorkspaceEntry::new(
        "ws1".to_string(),
        repo_root.join("missing"),
        "sv/ws/ws1".to_string(),
        "main".to_string(),
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );

    let result = storage.add_workspace(entry);
    assert!(result.is_err());
}

#[test]
fn add_find_remove_workspace() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();
    let workspace_path = repo_root.join("worktrees/ws1");
    std::fs::create_dir_all(&workspace_path).unwrap();

    let entry = WorkspaceEntry::new(
        "ws1".to_string(),
        workspace_path.clone(),
        "sv/ws/ws1".to_string(),
        "main".to_string(),
        Some("agent1".to_string()),
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );

    storage.add_workspace(entry).unwrap();

    let found = storage.find_workspace("ws1").unwrap().unwrap();
    assert_eq!(found.path, workspace_path);

    let removed = storage.remove_workspace("ws1").unwrap();
    assert!(removed.is_some());
    assert!(storage.find_workspace("ws1").unwrap().is_none());
}

#[test]
fn cleanup_stale_workspace_removes_missing_paths() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();
    let workspace_path = repo_root.join("worktrees/stale");
    std::fs::create_dir_all(&workspace_path).unwrap();

    let entry = WorkspaceEntry::new(
        "stale".to_string(),
        workspace_path.clone(),
        "sv/ws/stale".to_string(),
        "main".to_string(),
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );
    storage.add_workspace(entry).unwrap();

    std::fs::remove_dir_all(&workspace_path).unwrap();
    let removed = storage.cleanup_stale_workspaces().unwrap();
    assert_eq!(removed, 1);
    assert!(storage.find_workspace("stale").unwrap().is_none());
}

#[test]
fn update_workspace_mutates_fields() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();
    let workspace_path = repo_root.join("worktrees/ws2");
    std::fs::create_dir_all(&workspace_path).unwrap();

    let entry = WorkspaceEntry::new(
        "ws2".to_string(),
        workspace_path,
        "sv/ws/ws2".to_string(),
        "main".to_string(),
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );
    storage.add_workspace(entry).unwrap();

    storage
        .update_workspace("ws2", |entry| {
            entry.last_active = Some("2024-02-01T00:00:00Z".to_string());
            Ok(())
        })
        .unwrap();

    let updated = storage.find_workspace("ws2").unwrap().unwrap();
    assert_eq!(
        updated.last_active.as_deref(),
        Some("2024-02-01T00:00:00Z")
    );
}
