mod support;

use assert_cmd::Command;
use predicates::str::contains;

use support::TestRepo;

fn setup_repo() -> TestRepo {
    let repo = TestRepo::init().expect("init repo");
    repo.init_sv_dirs().expect("init sv dirs");
    repo
}

#[test]
#[ignore = "sv take not implemented yet"]
fn take_creates_lease() {
    let repo = setup_repo();

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args([
            "take",
            "src/lib.rs",
            "--strength",
            "cooperative",
            "--intent",
            "refactor",
            "--note",
            "test",
        ])
        .assert()
        .success();
}

#[test]
#[ignore = "sv lease ls not implemented yet"]
fn lease_ls_lists_active_leases() {
    let repo = setup_repo();

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["lease", "ls"])
        .assert()
        .success();
}

#[test]
#[ignore = "sv lease who not implemented yet"]
fn lease_who_reports_holders() {
    let repo = setup_repo();

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["lease", "who", "src/lib.rs"])
        .assert()
        .success()
        .stdout(contains("src/lib.rs"));
}

#[test]
#[ignore = "sv release not implemented yet"]
fn release_clears_lease() {
    let repo = setup_repo();

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["release", "src/lib.rs"])
        .assert()
        .success();
}

#[test]
#[ignore = "sv lease renew not implemented yet"]
fn lease_renew_extends_ttl() {
    let repo = setup_repo();

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["lease", "renew", "lease-id-1", "--ttl", "3h"])
        .assert()
        .success();
}

#[test]
#[ignore = "sv lease break not implemented yet"]
fn lease_break_requires_reason() {
    let repo = setup_repo();

    Command::cargo_bin("sv")
        .expect("binary")
        .current_dir(repo.path())
        .args(["lease", "break", "lease-id-1", "--reason", "test"])
        .assert()
        .success();
}
