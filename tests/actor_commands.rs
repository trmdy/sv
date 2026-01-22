mod support;

use std::fs;

use assert_cmd::Command;
use predicates::str::contains;

use support::TestRepo;

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

#[test]
fn actor_show_uses_env_when_set() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd(&repo)
        .env("SV_ACTOR", "env-actor")
        .args(["actor", "show"])
        .assert()
        .success()
        .stdout(contains("env-actor"));

    Ok(())
}

#[test]
fn actor_set_persists_and_show_reads() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd(&repo)
        .env_remove("SV_ACTOR")
        .args(["actor", "set", "persisted-actor"])
        .assert()
        .success();

    let actor_path = repo.path().join(".sv").join("actor");
    let contents = fs::read_to_string(actor_path)?;
    assert!(contents.contains("persisted-actor"));

    sv_cmd(&repo)
        .env_remove("SV_ACTOR")
        .args(["actor", "show"])
        .assert()
        .success()
        .stdout(contains("persisted-actor"));

    Ok(())
}

#[test]
fn actor_show_falls_back_to_config_default() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_sv_config("[actor]\ndefault = \"config-actor\"\n")?;

    sv_cmd(&repo)
        .env_remove("SV_ACTOR")
        .args(["actor", "show"])
        .assert()
        .success()
        .stdout(contains("config-actor"));

    Ok(())
}
