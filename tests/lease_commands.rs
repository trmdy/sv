mod support;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::thread::sleep;
use std::time::Duration;

use assert_cmd::Command;
use support::TestRepo;

fn setup_repo() -> TestRepo {
    let repo = TestRepo::init().expect("init repo");
    repo.init_sv_dirs().expect("init sv dirs");
    repo
}

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = support::sv_cmd();
    cmd.current_dir(repo.path());
    cmd
}

#[test]
fn take_creates_lease() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
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

    let leases = repo.read_leases().expect("read leases");
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].pathspec, "src/lib.rs");
}

#[test]
fn lease_ls_lists_active_leases() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
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

    sv_cmd(&repo)
        .args(["lease", "ls"])
        .assert()
        .success()
        .stdout(contains("src/lib.rs").and(contains("by alice")));
}

#[test]
fn lease_who_reports_holders() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
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

    sv_cmd(&repo)
        .args(["lease", "who", "src/lib.rs"])
        .assert()
        .success()
        .stdout(contains("Leases on 'src/lib.rs'").and(contains("by alice")));
}

#[test]
fn release_clears_lease() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
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

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
        .args(["release", "src/lib.rs"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["lease", "ls"])
        .assert()
        .success()
        .stdout(contains("No active leases."));
}

#[test]
fn lease_renew_extends_ttl() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
        .args([
            "take",
            "src/ttl.rs",
            "--strength",
            "cooperative",
            "--intent",
            "refactor",
            "--ttl",
            "1h",
        ])
        .assert()
        .success();

    let leases = repo.read_leases().expect("read leases");
    let lease = leases.first().expect("lease exists");
    let lease_id = lease.id.to_string();
    let old_expires = lease.expires_at;

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
        .args(["lease", "renew", &lease_id, "--ttl", "3h"])
        .assert()
        .success();

    let leases = repo.read_leases().expect("read leases");
    let renewed = leases
        .iter()
        .find(|l| l.id.to_string() == lease_id)
        .expect("renewed lease");
    assert_eq!(renewed.ttl, "3h");
    assert!(renewed.expires_at > old_expires);
}

#[test]
fn lease_break_marks_inactive() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
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

    let leases = repo.read_leases().expect("read leases");
    let lease_id = leases.first().expect("lease exists").id.to_string();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
        .args(["lease", "break", &lease_id, "--reason", "test"])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["lease", "ls"])
        .assert()
        .success()
        .stdout(contains("No active leases."));
}

#[test]
fn take_conflict_blocks_second_actor() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
        .args([
            "take",
            "src/conflict.rs",
            "--strength",
            "exclusive",
            "--intent",
            "refactor",
            "--note",
            "lock",
        ])
        .assert()
        .success();

    sv_cmd(&repo)
        .env("SV_ACTOR", "bob")
        .args([
            "take",
            "src/conflict.rs",
            "--strength",
            "cooperative",
            "--intent",
            "refactor",
            "--note",
            "try",
        ])
        .assert()
        .failure()
        .stderr(contains("Lease conflict"));
}

#[test]
fn lease_expiration_hides_from_list() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .env("SV_ACTOR", "alice")
        .args([
            "take",
            "src/ttl.rs",
            "--strength",
            "cooperative",
            "--intent",
            "refactor",
            "--ttl",
            "1s",
        ])
        .assert()
        .success();

    sleep(Duration::from_secs(2));

    sv_cmd(&repo)
        .args(["lease", "ls"])
        .assert()
        .success()
        .stdout(contains("No active leases."));
}

#[test]
fn ownerless_lease_is_labeled() {
    let repo = setup_repo();

    sv_cmd(&repo)
        .args([
            "take",
            "src/ownerless.rs",
            "--strength",
            "cooperative",
            "--intent",
            "docs",
        ])
        .assert()
        .success();

    sv_cmd(&repo)
        .args(["lease", "ls"])
        .assert()
        .success()
        .stdout(contains("(ownerless)"));
}
