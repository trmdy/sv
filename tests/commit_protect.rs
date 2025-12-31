mod support;

use assert_cmd::Command;
use predicates::str::contains;

use support::TestRepo;

#[test]
#[ignore = "commit wrapper not implemented yet"]
fn commit_injects_change_id() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("README.md", "# sv\n")?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("commit")
        .arg("-m")
        .arg("test change")
        .assert()
        .success();

    Ok(())
}

#[test]
#[ignore = "protect/commit enforcement not implemented yet"]
fn protected_path_blocks_commit() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file(".sv.toml", "[protect]\nmode = \"guard\"\npaths = [\".beads/**\"]\n")?;
    repo.write_file(".beads/issues.jsonl", "[]\n")?;

    Command::cargo_bin("sv")?
        .current_dir(repo.path())
        .arg("commit")
        .arg("-m")
        .arg("commit protected")
        .assert()
        .failure()
        .stderr(contains("Protected path"));

    Ok(())
}
