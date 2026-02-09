mod support;

use std::path::PathBuf;
use std::process::Output;
use std::sync::{Arc, Barrier};
use std::thread;

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;

use support::TestRepo;

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

fn sv_bin() -> PathBuf {
    assert_cmd::cargo::cargo_bin!("sv").to_path_buf()
}

fn setup_repo() -> Result<TestRepo, Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.commit_file("README.md", "base\n", "initial commit")?;
    sv_cmd(&repo).arg("init").assert().success();
    sv_cmd(&repo)
        .args(["ws", "here", "--name", "local"])
        .assert()
        .success();
    Ok(repo)
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

fn task_show_json(repo: &TestRepo, task_id: &str) -> Value {
    let output = sv_cmd(repo)
        .args(["task", "show", task_id, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("task show json")
}

#[test]
fn task_start_blocks_other_actor_without_takeover() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let task_id = new_task(&repo, "exclusive start");

    sv_cmd(&repo)
        .args(["--actor", "alice", "task", "start", &task_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["--actor", "bob", "task", "start", &task_id])
        .assert()
        .failure()
        .stderr(contains(
            "task already in progress by alice; use --takeover to transfer ownership",
        ));

    Ok(())
}

#[test]
fn task_start_takeover_transfers_ownership() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let task_id = new_task(&repo, "takeover");

    sv_cmd(&repo)
        .args(["--actor", "alice", "task", "start", &task_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["--actor", "bob", "task", "start", &task_id, "--takeover"])
        .assert()
        .success();

    let value = task_show_json(&repo, &task_id);
    assert_eq!(value["data"]["task"]["started_by"].as_str(), Some("bob"));
    assert_eq!(value["data"]["events"].as_u64(), Some(3));

    Ok(())
}

#[test]
fn task_start_same_actor_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let task_id = new_task(&repo, "idempotent");

    sv_cmd(&repo)
        .args(["--actor", "alice", "task", "start", &task_id])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["--actor", "alice", "task", "start", &task_id])
        .assert()
        .success()
        .stdout(contains("already in progress by you"));

    let value = task_show_json(&repo, &task_id);
    assert_eq!(value["data"]["task"]["started_by"].as_str(), Some("alice"));
    assert_eq!(value["data"]["events"].as_u64(), Some(2));

    Ok(())
}

#[test]
fn task_start_parallel_only_one_actor_wins() -> Result<(), Box<dyn std::error::Error>> {
    let repo = setup_repo()?;
    let task_id = new_task(&repo, "race");
    let repo_path = repo.path().to_path_buf();
    let bin = Arc::new(sv_bin());
    let barrier = Arc::new(Barrier::new(2));

    let mut handles = Vec::new();
    for actor in ["alice", "bob"] {
        let repo_path = repo_path.clone();
        let task_id = task_id.clone();
        let barrier = Arc::clone(&barrier);
        let bin = Arc::clone(&bin);
        let actor = actor.to_string();
        handles.push(thread::spawn(move || {
            barrier.wait();
            std::process::Command::new(bin.as_ref())
                .current_dir(&repo_path)
                .env("SV_ACTOR", &actor)
                .args(["task", "start", &task_id])
                .output()
                .expect("parallel task start")
        }));
    }

    let outputs: Vec<Output> = handles
        .into_iter()
        .map(|handle| handle.join().expect("join"))
        .collect();
    let success_count = outputs
        .iter()
        .filter(|output| output.status.success())
        .count();
    assert_eq!(success_count, 1);

    let failure = outputs
        .iter()
        .find(|output| !output.status.success())
        .expect("one failure");
    let stderr = String::from_utf8_lossy(&failure.stderr);
    assert!(stderr.contains("task already in progress by"));

    let value = task_show_json(&repo, &task_id);
    let owner = value["data"]["task"]["started_by"]
        .as_str()
        .expect("started_by");
    assert!(owner == "alice" || owner == "bob");
    assert_eq!(value["data"]["events"].as_u64(), Some(2));

    Ok(())
}
