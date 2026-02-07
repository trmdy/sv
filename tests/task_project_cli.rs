mod support;

use assert_cmd::Command;
use serde_json::Value;

use support::TestRepo;

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

fn new_task(repo: &TestRepo, title: &str) -> String {
    let output = sv_cmd(repo)
        .args(["task", "new", title, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("task new json");
    value["data"]["id"].as_str().expect("task id").to_string()
}

#[test]
fn task_project_set_clear_and_filters() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let project_id = new_task(&repo, "Project");
    let child_id = new_task(&repo, "Child");

    sv_cmd(&repo)
        .args(["task", "project", "set", &child_id, &project_id])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["task", "count", "--project", &project_id, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output)?;
    assert_eq!(value["data"]["total"].as_u64(), Some(2));

    sv_cmd(&repo)
        .args(["task", "project", "clear", &child_id])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["task", "count", "--project", &project_id, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output)?;
    assert_eq!(value["data"]["total"].as_u64(), Some(1));

    Ok(())
}

#[test]
fn task_project_filter_env_defaults_for_count() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let project_id = new_task(&repo, "Project");
    let child_id = new_task(&repo, "Child");
    let _other_id = new_task(&repo, "Other");

    sv_cmd(&repo)
        .args(["task", "project", "set", &child_id, &project_id])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["task", "count", "--json"])
        .env("SV_PROJECT", &project_id)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output)?;
    assert_eq!(value["data"]["total"].as_u64(), Some(2));

    Ok(())
}
