mod support;

use assert_cmd::Command;
use predicates::str::{contains, is_match};
use serde_json::Value;

use support::TestRepo;

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

#[test]
fn task_count_counts_with_filters() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd(&repo)
        .args(["task", "new", "One", "--priority", "P1"])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "new", "Two", "--priority", "P2"])
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "new", "Three", "--status", "closed"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "count"])
        .assert()
        .success()
        .stdout("3\n");

    sv_cmd(&repo)
        .args(["task", "count", "--status", "open"])
        .assert()
        .success()
        .stdout("2\n");

    sv_cmd(&repo)
        .args(["task", "count", "--ready"])
        .assert()
        .success()
        .stdout("2\n");

    sv_cmd(&repo)
        .args(["task", "count", "--priority", "P1"])
        .assert()
        .success()
        .stdout("1\n");

    sv_cmd(&repo)
        .args(["task", "count", "--limit", "1"])
        .assert()
        .success()
        .stdout("1\n");

    sv_cmd(&repo)
        .args(["task", "count", "--json"])
        .assert()
        .success()
        .stdout(contains("\"command\": \"task count\""))
        .stdout(is_match("(?s)\"data\"\\s*:\\s*\\{\\s*\"total\"\\s*:\\s*3\\s*\\}").unwrap());

    Ok(())
}

#[test]
fn task_count_rejects_status_with_ready() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd(&repo)
        .args(["task", "new", "One"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["task", "count", "--ready", "--status", "open"])
        .assert()
        .failure()
        .stderr(contains("cannot use --status with --ready"));

    Ok(())
}

#[test]
fn task_queries_do_not_use_sv_actor_as_filter() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd(&repo)
        .args(["task", "new", "Alice task"])
        .env("SV_ACTOR", "alice")
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "new", "Bob task"])
        .env("SV_ACTOR", "bob")
        .assert()
        .success();

    // SV_ACTOR is command identity, not an implicit query filter.
    sv_cmd(&repo)
        .args(["task", "count", "--status", "open"])
        .env("SV_ACTOR", "alice")
        .assert()
        .success()
        .stdout("2\n");

    let list_output = sv_cmd(&repo)
        .args(["task", "list", "--json"])
        .env("SV_ACTOR", "alice")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_value: Value = serde_json::from_slice(&list_output)?;
    assert_eq!(list_value["data"]["total"].as_u64(), Some(2));

    let ready_output = sv_cmd(&repo)
        .args(["task", "ready", "--json"])
        .env("SV_ACTOR", "alice")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_value: Value = serde_json::from_slice(&ready_output)?;
    assert_eq!(ready_value["data"]["total"].as_u64(), Some(2));

    Ok(())
}

#[test]
fn task_queries_support_sv_actor_filter_env() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd(&repo)
        .args(["task", "new", "Alice task"])
        .env("SV_ACTOR", "alice")
        .assert()
        .success();
    sv_cmd(&repo)
        .args(["task", "new", "Bob task"])
        .env("SV_ACTOR", "bob")
        .assert()
        .success();

    let alice_count = sv_cmd(&repo)
        .args(["task", "count", "--status", "open", "--json"])
        .env("SV_ACTOR_FILTER", "alice")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let alice_value: Value = serde_json::from_slice(&alice_count)?;
    assert_eq!(alice_value["data"]["total"].as_u64(), Some(1));

    // Explicit --updated-by should override SV_ACTOR_FILTER.
    let bob_count = sv_cmd(&repo)
        .args([
            "task",
            "count",
            "--status",
            "open",
            "--updated-by",
            "bob",
            "--json",
        ])
        .env("SV_ACTOR_FILTER", "alice")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let bob_value: Value = serde_json::from_slice(&bob_count)?;
    assert_eq!(bob_value["data"]["total"].as_u64(), Some(1));

    Ok(())
}
