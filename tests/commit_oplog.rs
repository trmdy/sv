mod support;

use assert_cmd::Command;
use predicates::str::contains;

use support::TestRepo;

#[test]
#[ignore = "sv commit/oplog not implemented yet"]
fn commit_writes_oplog_entry() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file("README.md", "# sv\n")?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("commit")
        .arg("-m")
        .arg("test commit")
        .assert()
        .success();

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("op")
        .arg("log")
        .assert()
        .success()
        .stdout(contains("commit"));

    Ok(())
}
