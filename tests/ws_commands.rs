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
    let mut cmd = support::sv_cmd();
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
    let expected = std::fs::canonicalize(&worktree_path)?;
    let actual = std::fs::canonicalize(&entry.path)?;
    assert_eq!(actual, expected);

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
    let expected = std::fs::canonicalize(repo.path())?;
    let actual = std::fs::canonicalize(&entry.path)?;
    assert_eq!(actual, expected);

    Ok(())
}

#[test]
fn ws_clean_removes_merged_workspaces() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let ws1_path = repo.path().join(".sv/worktrees/ws1");
    let ws2_path = repo.path().join(".sv/worktrees/ws2");

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["ws", "new", "ws2", "--base", "HEAD"])
        .assert()
        .success();

    std::fs::write(ws2_path.join("feature.txt"), "feature\n")?;
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&ws2_path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "feature work"])
        .current_dir(&ws2_path)
        .output()?;

    let output = sv_cmd(&repo)
        .args(["ws", "clean", "--dest", "HEAD", "--json"])
        .output()?;
    assert!(output.status.success());

    let value: Value = serde_json::from_slice(&output.stdout)?;
    let removed = value["cleanup"]["removed"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let removed_names: Vec<_> = removed
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    assert!(removed_names.contains(&"ws1".to_string()));
    assert!(!removed_names.contains(&"ws2".to_string()));

    assert!(!ws1_path.exists());
    assert!(ws2_path.exists());

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let registry = storage.read_workspaces()?;
    assert!(registry.find("ws1").is_none());
    assert!(registry.find("ws2").is_some());

    Ok(())
}

#[test]
fn ws_switch_resolves_workspace_path() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let ws1_path = repo.path().join(".sv/worktrees/ws1");

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["ws", "switch", "ws1", "--path"])
        .output()?;
    assert!(output.status.success());
    let actual_path = std::path::PathBuf::from(String::from_utf8(output.stdout)?.trim());
    assert_eq!(
        std::fs::canonicalize(actual_path)?,
        std::fs::canonicalize(ws1_path)?
    );

    Ok(())
}

#[test]
fn ws_switch_prompts_when_name_missing() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let ws2_path = repo.path().join(".sv/worktrees/ws2");

    sv_cmd(&repo)
        .args(["ws", "new", "ws1", "--base", "HEAD"])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["ws", "new", "ws2", "--base", "HEAD"])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["ws", "switch", "--path"])
        .write_stdin("2\n")
        .output()?;

    assert!(output.status.success());
    let actual_path = std::path::PathBuf::from(String::from_utf8(output.stdout)?.trim());
    assert_eq!(
        std::fs::canonicalize(actual_path)?,
        std::fs::canonicalize(ws2_path)?
    );

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let registry = storage.read_workspaces()?;
    let ws2 = registry.find("ws2").expect("workspace ws2");
    assert!(ws2.last_active.is_some());

    Ok(())
}
