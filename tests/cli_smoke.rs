use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;

#[test]
fn sv_help_works() {
    cargo_bin_cmd!("sv")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Simultaneous Versioning"));
}

#[test]
fn subcommand_help_works() {
    let subcommands = [
        "ws", "lease", "protect", "commit", "risk", "op", "undo", "actor", "init", "status",
    ];

    for cmd in subcommands {
        cargo_bin_cmd!("sv")
            .arg(cmd)
            .arg("--help")
            .assert()
            .success();
    }
}

#[test]
fn task_robot_help_works() {
    cargo_bin_cmd!("sv")
        .arg("task")
        .arg("--robot-help")
        .assert()
        .success()
        .stdout(contains("sv task --robot-help"))
        .stdout(contains("sv task new"));
}

#[test]
fn ws_robot_help_works() {
    cargo_bin_cmd!("sv")
        .arg("ws")
        .arg("--robot-help")
        .assert()
        .success()
        .stdout(contains("sv ws --robot-help"))
        .stdout(contains("sv ws new"));
}
