mod support;

use assert_cmd::Command;
use predicates::str::contains;

use support::TestRepo;
use sv::oplog::{OpLog, OpOutcome, OpRecord};
use sv::storage::Storage;

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
fn op_log_lists_recent_ops() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let log = OpLog::for_storage(&storage);
    let mut record = OpRecord::new("sv ws new agent1", Some("agent1".to_string()));
    record.outcome = OpOutcome::success();
    record.affected_workspaces.push("agent1".to_string());
    record.affected_refs.push("refs/heads/sv/ws/agent1".to_string());
    log.append(&record)?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("op")
        .arg("log")
        .arg("--limit")
        .arg("1")
        .assert()
        .success()
        .stdout(contains("actor=agent1"))
        .stdout(contains("sv ws new agent1"));

    Ok(())
}
