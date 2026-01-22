mod support;

use assert_cmd::Command;
use support::TestRepo;
use sv::storage::{ProtectOverride, Storage};

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

#[test]
fn protect_off_disables_pattern_in_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_sv_config(
        r#"
[protect]
mode = "guard"
paths = ["Cargo.lock", ".beads/**"]
"#,
    )?;

    sv_cmd(&repo)
        .args(["protect", "off", "Cargo.lock"])
        .assert()
        .success();

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let overrides: ProtectOverride = storage.read_json(&storage.protect_override_file())?;
    assert!(overrides.disabled_patterns.contains(&"Cargo.lock".to_string()));

    Ok(())
}
