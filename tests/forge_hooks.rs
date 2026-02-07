use std::fs;

use predicates::str::contains;

mod support;

use support::{sv_cmd, TestRepo};

#[test]
fn forge_hooks_install_writes_config() {
    let repo = TestRepo::init().expect("repo");
    repo.commit_file("README.md", "test\n", "init")
        .expect("commit");

    sv_cmd()
        .current_dir(repo.path())
        .args(["forge", "hooks", "install", "--loop", "review-loop"])
        .assert()
        .success()
        .stdout(contains("Forge hooks installed"));

    let config = fs::read_to_string(repo.path().join(".sv.toml")).expect("read config");
    assert!(config.contains("[integrations.forge]"));
    assert!(config.contains("enabled = true"));
    assert!(config.contains("loop_ref = \"review-loop\""));
    assert!(config.contains("[integrations.forge.on_task_start]"));
    assert!(config.contains("forge work set {task_id}"));
    assert!(config.contains("[integrations.forge.on_task_close]"));
    assert!(config.contains("forge work clear --loop {loop_ref}"));
}

#[test]
fn task_start_and_close_run_forge_hooks_best_effort() {
    let repo = TestRepo::init().expect("repo");
    repo.commit_file("README.md", "test\n", "init")
        .expect("commit");

    // Minimal init so ws/task commands have their dirs.
    sv_cmd()
        .current_dir(repo.path())
        .arg("init")
        .assert()
        .success();

    // Register current repo as a workspace so `sv task start` can attach ws metadata.
    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "alice")
        .args(["ws", "here", "--name", "local"])
        .assert()
        .success();

    // Hook cmds are arbitrary shell snippets; forge binary not required for this test.
    repo.write_sv_config(
        r#"
[integrations.forge]
enabled = true
loop_ref = "{actor}"

[integrations.forge.on_task_start]
cmd = "printf 'start:{task_id}:{actor}:{loop_ref}\\n' >> hooks.txt"

[integrations.forge.on_task_close]
cmd = "printf 'close:{task_id}:{actor}:{loop_ref}\\n' >> hooks.txt"
"#
        .trim(),
    )
    .expect("write config");

    let new_out = sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "alice")
        .args(["task", "new", "T1", "--json"])
        .output()
        .expect("task new");
    assert!(new_out.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&new_out.stdout).expect("json");
    let id = envelope["data"]["id"].as_str().expect("id").to_string();

    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "alice")
        .args(["task", "start", &id])
        .assert()
        .success();

    sv_cmd()
        .current_dir(repo.path())
        .env("SV_ACTOR", "alice")
        .args(["task", "close", &id])
        .assert()
        .success();

    let hooks = fs::read_to_string(repo.path().join("hooks.txt")).expect("read hooks");
    assert!(hooks.contains(&format!("start:{id}:alice:alice\n")));
    assert!(hooks.contains(&format!("close:{id}:alice:alice\n")));
}
