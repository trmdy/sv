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
        cargo_bin_cmd!("sv")
            .arg(cmd)
            .arg("--help")
            .assert()
            .success();
    }
}
