use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn sv_help_works() {
    Command::cargo_bin("sv")
        .expect("binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Simultaneous Versioning"));
}

#[test]
fn subcommand_help_works() {
    let subcommands = [
        "ws",
        "lease",
        "protect",
        "commit",
        "risk",
        "op",
        "undo",
        "actor",
        "init",
        "status",
    ];

    for cmd in subcommands {
        Command::cargo_bin("sv")
            .expect("binary")
            .arg(cmd)
            .arg("--help")
            .assert()
            .success();
    }
}
