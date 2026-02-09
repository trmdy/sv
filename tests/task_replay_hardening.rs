mod support;

use assert_cmd::Command;
use chrono::Utc;
use serde_json::Value;
use sv::task::{TaskEvent, TaskEventType};

use support::TestRepo;

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

fn write_task_log(repo: &TestRepo, events: &[TaskEvent]) -> Result<(), Box<dyn std::error::Error>> {
    let mut body = String::new();
    for event in events {
        body.push_str(&serde_json::to_string(event)?);
        body.push('\n');
    }
    repo.write_file(".tasks/tasks.jsonl", &body)?;
    repo.write_file(".git/sv/tasks.jsonl", &body)?;
    Ok(())
}

fn duplicate_create_fixture() -> Vec<TaskEvent> {
    let now = Utc::now();

    let mut create_a = TaskEvent::new(TaskEventType::TaskCreated, "sv-abc");
    create_a.event_id = "evt-001".to_string();
    create_a.title = Some("Primary".to_string());
    create_a.timestamp = now;

    let mut create_a_dupe = TaskEvent::new(TaskEventType::TaskCreated, "sv-abc");
    create_a_dupe.event_id = "evt-002".to_string();
    create_a_dupe.title = Some("Duplicate".to_string());
    create_a_dupe.timestamp = now + chrono::Duration::milliseconds(1);

    let mut create_project = TaskEvent::new(TaskEventType::TaskCreated, "sv-proj");
    create_project.event_id = "evt-003".to_string();
    create_project.title = Some("Project".to_string());
    create_project.timestamp = now + chrono::Duration::milliseconds(2);

    vec![create_a, create_a_dupe, create_project]
}

#[test]
fn duplicate_task_created_does_not_break_task_commands() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    write_task_log(&repo, &duplicate_create_fixture())?;
    sv_cmd(&repo).args(["task", "sync"]).assert().success();

    let list_output = sv_cmd(&repo)
        .args(["task", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list: Value = serde_json::from_slice(&list_output)?;
    assert_eq!(list["data"]["total"].as_u64(), Some(2));

    sv_cmd(&repo)
        .args(["task", "show", "sv-abc"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "project", "set", "sv-abc", "sv-proj"])
        .assert()
        .success();

    Ok(())
}

#[test]
fn task_doctor_and_repair_dedupe_duplicate_creates() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    write_task_log(&repo, &duplicate_create_fixture())?;

    let doctor_output = sv_cmd(&repo)
        .args(["task", "doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doctor: Value = serde_json::from_slice(&doctor_output)?;
    assert_eq!(
        doctor["data"]["duplicate_creates"]
            .as_array()
            .map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        doctor["data"]["duplicate_creates"][0]["task_id"].as_str(),
        Some("sv-abc")
    );
    assert_eq!(
        doctor["data"]["duplicate_creates"][0]["duplicate_event_ids"][0].as_str(),
        Some("evt-002")
    );

    let dry_run_output = sv_cmd(&repo)
        .args(["task", "repair", "--dedupe-creates", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dry_run: Value = serde_json::from_slice(&dry_run_output)?;
    assert_eq!(dry_run["data"]["removed_events"].as_u64(), Some(1));
    assert_eq!(dry_run["data"]["dry_run"].as_bool(), Some(true));

    sv_cmd(&repo)
        .args(["task", "repair", "--dedupe-creates"])
        .assert()
        .success();

    let doctor_after_output = sv_cmd(&repo)
        .args(["task", "doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doctor_after: Value = serde_json::from_slice(&doctor_after_output)?;
    assert_eq!(
        doctor_after["data"]["duplicate_creates"]
            .as_array()
            .map(|items| items.len()),
        Some(0)
    );

    Ok(())
}

#[test]
fn task_doctor_reports_malformed_events() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    let now = Utc::now();

    let mut create = TaskEvent::new(TaskEventType::TaskCreated, "sv-abc");
    create.event_id = "evt-001".to_string();
    create.title = Some("Primary".to_string());
    create.timestamp = now;

    let mut body = String::new();
    body.push_str(&serde_json::to_string(&create)?);
    body.push('\n');
    body.push_str("{not json}\n");
    repo.write_file(".tasks/tasks.jsonl", &body)?;

    let output = sv_cmd(&repo)
        .args(["task", "doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output)?;
    assert_eq!(
        value["data"]["malformed_events"]
            .as_array()
            .map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        value["data"]["malformed_events"][0]["line"].as_u64(),
        Some(2)
    );

    Ok(())
}

#[test]
fn task_sync_warns_on_duplicate_creates() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    write_task_log(&repo, &duplicate_create_fixture())?;

    let output = sv_cmd(&repo)
        .args(["task", "sync", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output)?;
    let warnings = value["warnings"].as_array().cloned().unwrap_or_default();
    assert!(warnings.iter().any(|entry| {
        entry
            .as_str()
            .map(|text| text.contains("duplicate task_created events detected"))
            .unwrap_or(false)
    }));

    Ok(())
}
