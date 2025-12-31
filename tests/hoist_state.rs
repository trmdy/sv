use std::fs;

use chrono::{DateTime, Utc};
use sv::storage::{
    HoistCommit, HoistCommitStatus, HoistConflict, HoistState, HoistStatus, Storage,
};
use tempfile::TempDir;

fn parse_ts(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("parse timestamp")
        .with_timezone(&Utc)
}

fn setup_storage() -> (TempDir, Storage) {
    let temp = TempDir::new().expect("temp dir");
    let repo_root = temp.path().to_path_buf();
    fs::create_dir(repo_root.join(".git")).expect("create .git");
    (temp, Storage::for_repo(repo_root))
}

#[test]
fn hoist_state_roundtrip() {
    let (_temp, storage) = setup_storage();

    let state = HoistState {
        hoist_id: "hoist-123".to_string(),
        dest_ref: "refs/heads/main".to_string(),
        integration_ref: "sv/hoist/main".to_string(),
        status: HoistStatus::InProgress,
        started_at: parse_ts("2025-01-01T12:00:00Z"),
        updated_at: parse_ts("2025-01-01T12:05:00Z"),
        commits: vec![HoistCommit {
            commit_id: "abc123".to_string(),
            status: HoistCommitStatus::Pending,
            workspace: Some("agent1".to_string()),
            change_id: Some("change-1".to_string()),
            summary: Some("Add auth guard".to_string()),
        }],
    };

    storage.write_hoist_state(&state).expect("write state");
    let loaded = storage
        .read_hoist_state("refs/heads/main")
        .expect("read state")
        .expect("state exists");

    assert_eq!(state, loaded);
}

#[test]
fn hoist_conflicts_roundtrip() {
    let (_temp, storage) = setup_storage();

    let record = HoistConflict {
        hoist_id: "hoist-456".to_string(),
        commit_id: "def456".to_string(),
        files: vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        message: Some("merge conflict".to_string()),
        recorded_at: parse_ts("2025-02-02T10:00:00Z"),
    };

    storage
        .append_hoist_conflict("refs/heads/main", &record)
        .expect("append conflict");

    let conflicts = storage
        .read_hoist_conflicts("refs/heads/main")
        .expect("read conflicts");

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0], record);
}

#[test]
fn hoist_clear_removes_state_and_conflicts() {
    let (_temp, storage) = setup_storage();

    let state = HoistState {
        hoist_id: "hoist-789".to_string(),
        dest_ref: "refs/heads/main".to_string(),
        integration_ref: "sv/hoist/main".to_string(),
        status: HoistStatus::Completed,
        started_at: parse_ts("2025-03-03T08:00:00Z"),
        updated_at: parse_ts("2025-03-03T08:10:00Z"),
        commits: vec![],
    };

    let record = HoistConflict {
        hoist_id: "hoist-789".to_string(),
        commit_id: "ghi789".to_string(),
        files: vec![],
        message: None,
        recorded_at: parse_ts("2025-03-03T08:05:00Z"),
    };

    storage.write_hoist_state(&state).expect("write state");
    storage
        .append_hoist_conflict("refs/heads/main", &record)
        .expect("append conflict");

    storage
        .clear_hoist_state("refs/heads/main")
        .expect("clear state");

    let state_after = storage
        .read_hoist_state("refs/heads/main")
        .expect("read state");
    let conflicts_after = storage
        .read_hoist_conflicts("refs/heads/main")
        .expect("read conflicts");

    assert!(state_after.is_none());
    assert!(conflicts_after.is_empty());
}
