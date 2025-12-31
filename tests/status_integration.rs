mod support;

use assert_cmd::Command;

use support::TestRepo;

#[test]
fn status_runs_in_repo() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("status")
        .assert()
        .success();

    Ok(())
}
