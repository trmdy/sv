mod support;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;

use support::TestRepo;
use sv::storage::Storage;

fn setup_repo() -> Result<TestRepo, Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.commit_file("README.md", "base\n", "initial commit")?;
    Ok(repo)
}

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = Command::cargo_bin("sv").expect("binary");
    cmd.current_dir(repo.path());
    cmd
}

#[test]
fn ws_new_creates_worktree_and_registry() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let worktree_path = repo.path().join(".sv/worktrees/ws1");

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();

    assert!(worktree_path.exists());

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let registry = storage.read_workspaces()?;
    let entry = registry.find("ws1").expect("workspace entry");
    assert_eq!(entry.path, worktree_path);

    Ok(())
}

#[test]
fn ws_list_reports_registered_workspaces() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["ws", "new", "ws2", "--base", "HEAD"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["ws", "list"])
        .assert()
        .success()
        .stdout(contains("ws1").and(contains("ws2")));

    Ok(())
}

#[test]
fn ws_info_reports_details() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["ws", "info", "ws1", "--json"])
        .output()?;

    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["name"], "ws1");
    assert_eq!(value["exists"], true);

    Ok(())
}

#[test]
fn ws_rm_unregisters_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let worktree_path = repo.path().join(".sv/worktrees/ws1");

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();
    assert!(worktree_path.exists());

    sv_cmd(&repo)
        .args(["ws", "rm", "ws1", "--force"])
        .assert()
        .success();

    assert!(!worktree_path.exists());

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let registry = storage.read_workspaces()?;
    assert!(registry.find("ws1").is_none());

    Ok(())
}

#[test]
fn ws_here_registers_current_repo() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;

    sv_cmd(&repo)
        .args(["ws", "here", "--name", "current"])
        .assert()
        .success();

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let registry = storage.read_workspaces()?;
    let entry = registry.find("current").expect("workspace entry");
    assert_eq!(entry.path, repo.path().to_path_buf());

    Ok(())
}
