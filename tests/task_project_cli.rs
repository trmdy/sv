mod support;

use assert_cmd::Command;
use predicates::str::contains;
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

fn task_status(repo: &TestRepo, task_id: &str) -> String {
    let output = sv_cmd(repo)
        .args(["task", "show", task_id, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("task show json");
    value["data"]["task"]["status"]
        .as_str()
        .expect("task status")
        .to_string()
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

#[test]
fn task_project_filter_includes_tasks_via_epic_project_inheritance(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let project_id = new_task(&repo, "Project");
    let epic_id = new_task(&repo, "Epic");
    let child_id = new_task(&repo, "Child");

    sv_cmd(&repo)
        .args(["task", "project", "set", &epic_id, &project_id])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_id, &epic_id])
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
    assert_eq!(value["data"]["total"].as_u64(), Some(3));

    Ok(())
}

#[test]
fn project_groupings_cannot_be_closed_or_set_to_closed_status(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let project_id = new_task(&repo, "Project");
    let child_id = new_task(&repo, "Child");
    sv_cmd(&repo)
        .args(["task", "project", "set", &child_id, &project_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "close", &project_id])
        .assert()
        .failure()
        .stderr(contains("project groups cannot be completed"));

    sv_cmd(&repo)
        .args(["task", "status", &project_id, "closed"])
        .assert()
        .failure()
        .stderr(contains("project groups cannot be completed"));

    Ok(())
}

#[test]
fn parent_set_rejects_project_group_parent() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let project_id = new_task(&repo, "Project");
    let member_id = new_task(&repo, "Member");
    let child_id = new_task(&repo, "Child");

    sv_cmd(&repo)
        .args(["task", "project", "set", &member_id, &project_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "parent", "set", &child_id, &project_id])
        .assert()
        .failure()
        .stderr(contains("tasks cannot be children of project groups"));

    Ok(())
}

#[test]
fn project_set_rejects_legacy_project_with_children() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let project_id = new_task(&repo, "Project");
    let child_id = new_task(&repo, "Child");
    let member_id = new_task(&repo, "Member");

    sv_cmd(&repo)
        .args(["task", "parent", "set", &child_id, &project_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "project", "set", &member_id, &project_id])
        .assert()
        .failure()
        .stderr(contains(
            "project groups cannot have child tasks; clear parent links first",
        ));

    Ok(())
}

#[test]
fn epic_auto_closes_when_last_child_is_closed_via_repo_config(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_sv_config(
        "[tasks.epics]
auto_close_when_all_tasks_closed = true
",
    )?;

    let epic_id = new_task(&repo, "Epic");
    let child_a = new_task(&repo, "Child A");
    let child_b = new_task(&repo, "Child B");

    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_a, &epic_id])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_b, &epic_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "close", &child_a])
        .assert()
        .success();
    assert_eq!(task_status(&repo, &epic_id), "open");

    sv_cmd(&repo)
        .args(["task", "close", &child_b])
        .assert()
        .success();
    assert_eq!(task_status(&repo, &epic_id), "closed");

    Ok(())
}

#[test]
fn epic_auto_closes_when_global_env_is_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let epic_id = new_task(&repo, "Epic");
    let child_id = new_task(&repo, "Child");
    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_id, &epic_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .env("SV_TASK_EPIC_AUTO_CLOSE", "true")
        .args(["task", "close", &child_id])
        .assert()
        .success();

    assert_eq!(task_status(&repo, &epic_id), "closed");

    Ok(())
}

#[test]
fn epic_auto_close_per_epic_off_overrides_repo_on() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_sv_config(
        "[tasks.epics]
auto_close_when_all_tasks_closed = true
",
    )?;

    let epic_id = new_task(&repo, "Epic");
    let child_id = new_task(&repo, "Child");
    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_id, &epic_id])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "epic", "auto-close", &epic_id, "off"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "close", &child_id])
        .assert()
        .success();

    assert_eq!(task_status(&repo, &epic_id), "open");

    Ok(())
}

#[test]
fn epic_auto_close_per_epic_on_overrides_repo_off() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_sv_config(
        "[tasks.epics]
auto_close_when_all_tasks_closed = false
",
    )?;

    let epic_id = new_task(&repo, "Epic");
    let child_id = new_task(&repo, "Child");
    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_id, &epic_id])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "epic", "auto-close", &epic_id, "on"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "close", &child_id])
        .assert()
        .success();

    assert_eq!(task_status(&repo, &epic_id), "closed");

    Ok(())
}
