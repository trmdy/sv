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

fn new_project(repo: &TestRepo, name: &str) -> String {
    let output = sv_cmd(repo)
        .args(["project", "new", name, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("project new json");
    value["data"]["id"]
        .as_str()
        .expect("project id")
        .to_string()
}

#[test]
fn project_entities_can_group_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    let task_id = new_task(&repo, "Child");
    let project_id = new_project(&repo, "Platform");

    sv_cmd(&repo)
        .args(["task", "project", "set", &task_id, &project_id])
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
fn migrate_legacy_creates_project_entities_for_task_backed_projects(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    let legacy_anchor = new_task(&repo, "Legacy project anchor");
    let child = new_task(&repo, "Child");

    sv_cmd(&repo)
        .args(["task", "project", "set", &child, &legacy_anchor])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["project", "migrate-legacy"])
        .assert()
        .success();

    let list = sv_cmd(&repo)
        .args(["project", "list", "--all", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&list)?;
    let projects = value["data"]["projects"].as_array().expect("project array");
    assert!(
        projects
            .iter()
            .any(|project| project["id"].as_str() == Some(legacy_anchor.as_str())),
        "expected migrated project id to match legacy anchor id"
    );

    Ok(())
}
