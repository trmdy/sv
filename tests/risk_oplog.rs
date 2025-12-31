mod support;

use assert_cmd::Command;
use chrono::Utc;
use predicates::str::contains;

use sv::oplog::{OpLog, OpRecord};
use support::TestRepo;

fn setup_repo() -> TestRepo {
    let repo = TestRepo::init().expect("init repo");
    repo.init_sv_dirs().expect("init sv dirs");
    std::fs::create_dir_all(repo.git_sv_dir().join("oplog")).expect("oplog dir");
    repo
}

#[test]
#[ignore = "sv op log CLI output not implemented yet"]
fn op_log_lists_entries() {
    let repo = setup_repo();

    let log = OpLog::new(repo.git_sv_dir().join("oplog"));
    let record = OpRecord::new("sv init", Some("tester".to_string()));
    log.append(&record).expect("append op");

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["op", "log", "--limit", "1"])
        .assert()
        .success()
        .stdout(contains("sv init"));
}

#[test]
#[ignore = "sv risk CLI output not implemented yet"]
fn risk_command_reports_overlap() {
    let repo = setup_repo();
    repo.commit_file("README.md", "# sv\n", "init")
        .expect("commit file");

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["risk", "--json"])
        .assert()
        .success()
        .stdout(contains("overlap"));
}

#[test]
#[ignore = "sv undo CLI output not implemented yet"]
fn undo_command_reverts_last_op() {
    let repo = setup_repo();
    let log = OpLog::new(repo.git_sv_dir().join("oplog"));
    let mut record = OpRecord::new("sv ws new test", Some("tester".to_string()));
    record.timestamp = Utc::now();
    log.append(&record).expect("append op");

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["undo"])
        .assert()
        .success();
}
