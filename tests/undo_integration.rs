mod support;

use assert_cmd::Command;

use support::TestRepo;

#[test]
#[ignore = "undo command not implemented yet"]
fn undo_latest_operation() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("undo")
        .assert()
        .success();

    Ok(())
}

#[test]
#[ignore = "undo command not implemented yet"]
fn undo_specific_operation() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("undo")
        .arg("--op")
        .arg("op-123")
        .assert()
        .failure();

    Ok(())
}
