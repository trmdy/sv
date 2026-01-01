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
    let workspace_path = repo_root.join(".sv/worktrees/ws1");
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
    let workspace_path = repo_root.join(".sv/worktrees/stale");
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
    let workspace_path = repo_root.join(".sv/worktrees/ws2");
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

#[test]
fn list_workspaces_returns_all() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();

    // Create multiple workspace directories
    let paths: Vec<_> = (1..=3)
        .map(|i| {
            let path = repo_root.join(format!(".sv/worktrees/ws{}", i));
            std::fs::create_dir_all(&path).unwrap();
            path
        })
        .collect();

    // Add workspaces to registry
    for (i, path) in paths.iter().enumerate() {
        let name = format!("ws{}", i + 1);
        let entry = WorkspaceEntry::new(
            name.clone(),
            path.clone(),
            format!("sv/ws/{}", name),
            "main".to_string(),
            None,
            "2024-01-01T00:00:00Z".to_string(),
            None,
        );
        storage.add_workspace(entry).unwrap();
    }

    let all = storage.list_workspaces().unwrap();
    assert_eq!(all.len(), 3);

    let names: Vec<_> = all.iter().map(|w| w.name.as_str()).collect();
    assert!(names.contains(&"ws1"));
    assert!(names.contains(&"ws2"));
    assert!(names.contains(&"ws3"));
}

#[test]
fn remove_nonexistent_workspace_returns_none() {
    let (_temp, storage) = setup_storage();
    let result = storage.remove_workspace("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn find_nonexistent_workspace_returns_none() {
    let (_temp, storage) = setup_storage();
    let result = storage.find_workspace("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn add_duplicate_workspace_fails() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();
    let workspace_path = repo_root.join(".sv/worktrees/dup");
    std::fs::create_dir_all(&workspace_path).unwrap();

    let entry = WorkspaceEntry::new(
        "dup".to_string(),
        workspace_path.clone(),
        "sv/ws/dup".to_string(),
        "main".to_string(),
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );
    storage.add_workspace(entry.clone()).unwrap();

    // Try to add again with same name
    let result = storage.add_workspace(entry);
    assert!(result.is_err());
}

#[test]
fn update_nonexistent_workspace_fails() {
    let (_temp, storage) = setup_storage();
    let result = storage.update_workspace("nonexistent", |_| Ok(()));
    assert!(result.is_err());
}

#[test]
fn workspace_entry_has_unique_id() {
    let (_temp, storage) = setup_storage();
    let repo_root = storage.local_dir().parent().unwrap().to_path_buf();

    let path1 = repo_root.join(".sv/worktrees/id1");
    let path2 = repo_root.join(".sv/worktrees/id2");
    std::fs::create_dir_all(&path1).unwrap();
    std::fs::create_dir_all(&path2).unwrap();

    let entry1 = WorkspaceEntry::new(
        "id1".to_string(),
        path1,
        "sv/ws/id1".to_string(),
        "main".to_string(),
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );
    let entry2 = WorkspaceEntry::new(
        "id2".to_string(),
        path2,
        "sv/ws/id2".to_string(),
        "main".to_string(),
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    );

    storage.add_workspace(entry1).unwrap();
    storage.add_workspace(entry2).unwrap();

    let ws1 = storage.find_workspace("id1").unwrap().unwrap();
    let ws2 = storage.find_workspace("id2").unwrap().unwrap();

    // Each should have a unique ID assigned (non-empty UUIDs)
    assert!(!ws1.id.is_empty());
    assert!(!ws2.id.is_empty());
    assert_ne!(ws1.id, ws2.id);
}
