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

fn status_count(stats: &Value, field: &str, status: &str) -> u64 {
    stats[field]
        .as_array()
        .expect("status array")
        .iter()
        .find(|entry| entry["status"].as_str() == Some(status))
        .and_then(|entry| entry["count"].as_u64())
        .unwrap_or(0)
}

#[test]
fn task_stats_reports_repo_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    let epic_id = new_task(&repo, "Epic");
    let child_id = new_task(&repo, "Child");
    let close_id = new_task(&repo, "To close");

    sv_cmd(&repo)
        .args(["task", "epic", "set", &child_id, &epic_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "close", &close_id])
        .assert()
        .success();

    let project_output = sv_cmd(&repo)
        .args(["project", "new", "Platform", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let project_value: Value = serde_json::from_slice(&project_output)?;
    let project_id = project_value["data"]["id"]
        .as_str()
        .expect("project id")
        .to_string();

    sv_cmd(&repo)
        .args(["task", "project", "set", &child_id, &project_id])
        .assert()
        .success();

    let output = sv_cmd(&repo)
        .args(["task", "stats", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output)?;
    let stats = &value["data"];

    assert_eq!(value["command"].as_str(), Some("task stats"));
    assert_eq!(stats["tasks_total"].as_u64(), Some(3));
    assert_eq!(stats["epics_total"].as_u64(), Some(1));
    assert_eq!(stats["project_entities_total"].as_u64(), Some(1));
    assert!(stats["project_groups_total"].as_u64().unwrap_or(0) >= 1);

    assert_eq!(status_count(stats, "task_statuses", "open"), 2);
    assert_eq!(status_count(stats, "task_statuses", "closed"), 1);

    let task_events = stats["task_events_total"].as_u64().unwrap_or(0);
    let project_events = stats["project_events_total"].as_u64().unwrap_or(0);
    let all_events = stats["events_total"].as_u64().unwrap_or(0);
    assert_eq!(all_events, task_events + project_events);

    assert!(
        stats["throughput_last_24_hours"]["tasks_completed"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );

    assert!(
        stats["compaction"]["before_events"].as_u64().unwrap_or(0)
            >= stats["compaction"]["after_events"].as_u64().unwrap_or(0)
    );

    Ok(())
}
