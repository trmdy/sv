mod support;

use assert_cmd::Command;
use predicates::str::{contains, is_match};

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
