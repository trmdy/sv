mod support;

use assert_cmd::Command;
use predicates::str::contains;

use support::TestRepo;

#[test]
#[ignore = "risk command not implemented yet"]
fn risk_reports_overlap() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("risk")
        .assert()
        .success();

    Ok(())
}

#[test]
#[ignore = "op log command not implemented yet"]
fn op_log_lists_recent_ops() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("op")
        .arg("log")
        .assert()
        .success()
        .stdout(contains("op"));

    Ok(())
}
