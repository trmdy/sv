mod support;

use support::TestRepo;
use sv::merge::{simulate_merge, MergeConflictKind};

#[test]
fn simulate_merge_detects_content_conflict() {
    let repo = TestRepo::init().expect("init repo");
    repo.write_file("file.txt", "base\n").expect("write base");
    repo.commit_all("base").expect("commit base");

    repo.create_branch("ours").expect("create ours");
    repo.create_branch("theirs").expect("create theirs");

    repo.checkout_branch("ours").expect("checkout ours");
    repo.write_file("file.txt", "ours\n").expect("write ours");
    repo.commit_all("ours").expect("commit ours");

    repo.checkout_branch("theirs").expect("checkout theirs");
    repo.write_file("file.txt", "theirs\n")
        .expect("write theirs");
    repo.commit_all("theirs").expect("commit theirs");

    let simulation = simulate_merge(repo.repo(), "refs/heads/ours", "refs/heads/theirs", None)
        .expect("simulate merge");

    assert!(
        simulation.conflicts.iter().any(|conflict| {
            conflict.path == "file.txt" && matches!(conflict.kind, MergeConflictKind::Content)
        }),
        "expected content conflict on file.txt"
    );
}

#[test]
fn simulate_merge_returns_no_conflicts_for_disjoint_changes() {
    let repo = TestRepo::init().expect("init repo");
    repo.write_file("file1.txt", "base\n").expect("write base");
    repo.commit_all("base").expect("commit base");

    repo.create_branch("ours").expect("create ours");
    repo.create_branch("theirs").expect("create theirs");

    repo.checkout_branch("ours").expect("checkout ours");
    repo.write_file("file1.txt", "ours\n").expect("write ours");
    repo.commit_all("ours").expect("commit ours");

    repo.checkout_branch("theirs").expect("checkout theirs");
    repo.write_file("file2.txt", "theirs\n")
        .expect("write theirs");
    repo.commit_all("theirs").expect("commit theirs");

    let simulation = simulate_merge(repo.repo(), "refs/heads/ours", "refs/heads/theirs", None)
        .expect("simulate merge");

    assert!(
        simulation.conflicts.is_empty(),
        "expected no conflicts for disjoint changes"
    );
}
