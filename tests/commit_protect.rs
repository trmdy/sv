mod support;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;

use support::{sv_cmd, TestRepo};

#[test]
fn commit_injects_change_id() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file("README.md", "# sv\n")?;
    repo.commit_all("initial commit")?;
    repo.write_file("README.md", "# sv v2\n")?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("commit")
        .arg("-a")
        .arg("-m")
        .arg("test change")
        .assert()
        .success();

    let commit = repo.repo().head()?.peel_to_commit()?;
    let message = commit.message().unwrap_or_default();
    assert!(message.contains("Change-Id:"));

    Ok(())
}

#[test]
fn protected_path_blocks_commit() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file(
        ".sv.toml",
        "[protect]\nmode = \"guard\"\npaths = [\".beads/**\"]\n",
    )?;
    repo.write_file(".beads/issues.jsonl", "[]\n")?;
    repo.commit_all("initial commit")?;
    repo.write_file(".beads/issues.jsonl", "[1]\n")?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("commit")
        .arg("-a")
        .arg("-m")
        .arg("commit protected")
        .assert()
        .failure()
        .stderr(contains("Protected path"));

    Ok(())
}

#[test]
fn allow_protected_overrides_guard() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file(
        ".sv.toml",
        "[protect]\nmode = \"guard\"\npaths = [\".beads/**\"]\n",
    )?;
    repo.write_file(".beads/issues.jsonl", "[]\n")?;
    repo.commit_all("initial commit")?;
    repo.write_file(".beads/issues.jsonl", "[1]\n")?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("commit")
        .arg("-a")
        .arg("-m")
        .arg("commit protected")
        .arg("--allow-protected")
        .assert()
        .success();

    Ok(())
}

#[test]
fn protect_add_and_rm_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;

    sv_cmd()
        .current_dir(repo.path())
        .args(["protect", "add", ".beads/**"])
        .assert()
        .success();

    let config_path = repo.path().join(".sv.toml");
    let contents = fs::read_to_string(&config_path)?;
    assert!(contents.contains(".beads/**"));

    sv_cmd()
        .current_dir(repo.path())
        .args(["protect", "rm", ".beads/**"])
        .assert()
        .success();

    let updated = fs::read_to_string(&config_path)?;
    assert!(!updated.contains(".beads/**"));

    Ok(())
}

#[test]
fn protect_status_reports_staged_matches() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("README.md", "base")?;
    repo.commit_all("initial commit")?;
    repo.write_file(
        ".sv.toml",
        "[protect]\nmode = \"guard\"\npaths = [\".beads/**\"]\n",
    )?;
    repo.write_file(".beads/issues.jsonl", "[]\n")?;
    repo.stage_path(".beads/issues.jsonl")?;

    sv_cmd()
        .current_dir(repo.path())
        .args(["protect", "status"])
        .assert()
        .success()
        .stdout(
            contains("staged files match protected patterns").and(contains(".beads/issues.jsonl")),
        );

    Ok(())
}

#[test]
fn lease_conflict_blocks_commit() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file("src/lib.rs", "fn main() {}\n")?;
    repo.commit_all("initial commit")?;
    repo.write_file("src/lib.rs", "fn main() { println!(\"hi\"); }\n")?;

    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "bob")
        .args([
            "take",
            "src/lib.rs",
            "--strength",
            "exclusive",
            "--intent",
            "refactor",
            "--note",
            "lock",
        ])
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "alice")
        .arg("commit")
        .arg("-a")
        .arg("-m")
        .arg("conflict commit")
        .assert()
        .failure()
        .stderr(contains("Lease conflict"));

    Ok(())
}

#[test]
fn force_lease_allows_commit() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file("src/lib.rs", "fn main() {}\n")?;
    repo.commit_all("initial commit")?;
    repo.write_file("src/lib.rs", "fn main() { println!(\"hi\"); }\n")?;

    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "bob")
        .args([
            "take",
            "src/lib.rs",
            "--strength",
            "exclusive",
            "--intent",
            "refactor",
            "--note",
            "lock",
        ])
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "alice")
        .arg("commit")
        .arg("-a")
        .arg("-m")
        .arg("force commit")
        .arg("--force-lease")
        .assert()
        .success();

    Ok(())
}
