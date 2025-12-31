mod support;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

use support::TestRepo;

#[test]
#[ignore = "lease commands not implemented yet"]
fn take_and_list_lease() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("take")
        .arg("src/lib.rs")
        .arg("--strength")
        .arg("exclusive")
        .arg("--note")
        .arg("test lease")
        .assert()
        .success();

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("lease")
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("exclusive"));

    Ok(())
}

#[test]
#[ignore = "lease commands not implemented yet"]
fn who_shows_active_lease() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("take")
        .arg("src/main.rs")
        .arg("--note")
        .arg("info")
        .assert()
        .success();

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("lease")
        .arg("who")
        .arg("src/main.rs")
        .assert()
        .success()
        .stdout(contains("src/main.rs"));

    Ok(())
}

#[test]
#[ignore = "lease commands not implemented yet"]
fn release_clears_lease() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("take")
        .arg("docs/**")
        .arg("--note")
        .arg("docs")
        .assert()
        .success();

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("release")
        .arg("docs/**")
        .assert()
        .success();

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("lease")
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("docs/**").not());

    Ok(())
}
