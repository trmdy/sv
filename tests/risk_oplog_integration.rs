mod support;

use std::fs;

use assert_cmd::Command;
use chrono::Utc;
use predicates::str::contains;
use serde_json::Value;

use support::TestRepo;
use sv::oplog::{OpLog, OpOutcome, OpRecord};
use sv::storage::{Storage, WorkspaceEntry};

#[test]
fn risk_reports_overlap() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    repo.write_file("shared.txt", "base\n")?;
    repo.commit_all("base commit")?;

    let git_repo = repo.repo();
    if git_repo.find_branch("main", git2::BranchType::Local).is_err() {
        repo.create_branch("main")?;
    }
    repo.checkout_branch("main")?;

    repo.create_branch("sv/ws/ws1")?;
    repo.checkout_branch("sv/ws/ws1")?;
    repo.write_file("shared.txt", "ws1 change\n")?;
    repo.commit_all("ws1 change")?;

    repo.checkout_branch("main")?;
    repo.create_branch("sv/ws/ws2")?;
    repo.checkout_branch("sv/ws/ws2")?;
    repo.write_file("shared.txt", "ws2 change\n")?;
    repo.commit_all("ws2 change")?;
    repo.checkout_branch("main")?;

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let worktrees_root = repo.path().join("worktrees");
    fs::create_dir_all(worktrees_root.join("ws1"))?;
    fs::create_dir_all(worktrees_root.join("ws2"))?;

    let now = Utc::now().to_rfc3339();
    storage.add_workspace(WorkspaceEntry::new(
        "ws1".to_string(),
        worktrees_root.join("ws1"),
        "sv/ws/ws1".to_string(),
        "main".to_string(),
        Some("agent1".to_string()),
        now.clone(),
        None,
    ))?;
    storage.add_workspace(WorkspaceEntry::new(
        "ws2".to_string(),
        worktrees_root.join("ws2"),
        "sv/ws/ws2".to_string(),
        "main".to_string(),
        Some("agent2".to_string()),
        now,
        None,
    ))?;

    let output = Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("risk")
        .arg("--json")
        .output()?;
    assert!(output.status.success());

    let report: Value = serde_json::from_slice(&output.stdout)?;
    let overlaps = report
        .get("overlaps")
        .and_then(|value| value.as_array())
        .expect("overlaps array");
    let overlap = overlaps
        .iter()
        .find(|entry| entry.get("path").and_then(|v| v.as_str()) == Some("shared.txt"))
        .expect("shared.txt overlap");
    let severity = overlap
        .get("severity")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(severity, "low");
    let workspaces = overlap
        .get("workspaces")
        .and_then(|value| value.as_array())
        .expect("workspaces array");
    assert!(workspaces.iter().any(|value| value.as_str() == Some("ws1")));
    assert!(workspaces.iter().any(|value| value.as_str() == Some("ws2")));

    Ok(())
}

#[test]
fn op_log_lists_recent_ops() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let log = OpLog::for_storage(&storage);
    let mut record = OpRecord::new("sv ws new agent1", Some("agent1".to_string()));
    record.outcome = OpOutcome::success();
    record.affected_workspaces.push("agent1".to_string());
    record.affected_refs.push("refs/heads/sv/ws/agent1".to_string());
    log.append(&record)?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("op")
        .arg("log")
        .arg("--limit")
        .arg("1")
        .assert()
        .success()
        .stdout(contains("actor=agent1"))
        .stdout(contains("sv ws new agent1"));

    Ok(())
}
