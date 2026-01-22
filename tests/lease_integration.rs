mod support;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

use support::{sv_cmd, TestRepo};

#[test]
fn take_and_list_lease() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("take")
        .arg("src/lib.rs")
        .arg("--strength")
        .arg("exclusive")
        .arg("--note")
        .arg("test lease")
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .arg("lease")
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("exclusive"));

    Ok(())
}

#[test]
fn who_shows_active_lease() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("take")
        .arg("src/main.rs")
        .arg("--note")
        .arg("info")
        .assert()
        .success();

    sv_cmd()
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
fn release_clears_lease() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("take")
        .arg("docs/**")
        .arg("--note")
        .arg("docs")
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .arg("release")
        .arg("docs/**")
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .arg("lease")
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("docs/**").not());

    Ok(())
}

#[test]
fn wait_until_lease_expires() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    sv_cmd()
        .current_dir(repo.path())
        .args(["take", "src/wait.rs", "--ttl", "1s"])
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .args([
            "lease",
            "wait",
            "src/wait.rs",
            "--timeout",
            "5s",
            "--poll",
            "1s",
        ])
        .assert()
        .success();

    Ok(())
}

#[test]
fn ownerless_lease_does_not_block_commit() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_sv_config("[actor]\ndefault = \"unknown\"\n")?;
    repo.write_file("src/ownerless.rs", "ownerless\n")?;
    repo.stage_path("src/ownerless.rs")?;

    sv_cmd()
        .current_dir(repo.path())
        .env_remove("SV_ACTOR")
        .args([
            "take",
            "src/ownerless.rs",
            "--strength",
            "exclusive",
            "--note",
            "ownerless lease",
        ])
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .env_remove("SV_ACTOR")
        .args(["commit", "-m", "ownerless commit"])
        .assert()
        .success();

    Ok(())
}
