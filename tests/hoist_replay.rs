use std::path::Path;
use std::process::Command;

use git2::{Oid, Repository};

use sv::hoist::{replay_commits, ReplayOptions};
use sv::storage::HoistCommitStatus;

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write_file(repo: &Path, path: &str, contents: &str) {
    std::fs::write(repo.join(path), contents).expect("write file");
}

fn commit_all(repo: &Path, message: &str) -> Oid {
    git(repo, &["add", "."]);
    git(repo, &["commit", "-m", message]);
    let oid = git_stdout(repo, &["rev-parse", "HEAD"]);
    Oid::from_str(&oid).expect("oid")
}

fn init_repo() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    let repo = Repository::open(dir.path()).expect("repo");
    (dir, repo)
}

fn tree_has_path(repo: &Repository, oid: Oid, path: &str) -> bool {
    let commit = match repo.find_commit(oid) {
        Ok(commit) => commit,
        Err(_) => return false,
    };
    let tree = match commit.tree() {
        Ok(tree) => tree,
        Err(_) => return false,
    };
    tree.get_path(std::path::Path::new(path)).is_ok()
}

fn setup_conflict_commits(repo: &Path) -> (String, Vec<Oid>) {
    write_file(repo, "file.txt", "base\n");
    commit_all(repo, "base");
    git(repo, &["branch", "feature"]);

    // Integration branch diverges with a conflicting change.
    write_file(repo, "file.txt", "main change\n");
    commit_all(repo, "main change");
    git(repo, &["branch", "integration"]);

    // Feature branch: conflicting change then a clean change.
    git(repo, &["checkout", "feature"]);
    write_file(repo, "file.txt", "feature change\n");
    let conflict_oid = commit_all(repo, "feature change");
    write_file(repo, "other.txt", "clean\n");
    let clean_oid = commit_all(repo, "feature clean");

    // Return to main for stable repo state.
    git(repo, &["checkout", "main"]);

    ("integration".to_string(), vec![conflict_oid, clean_oid])
}

#[test]
fn replay_commits_stops_on_conflict() {
    let (dir, repo) = init_repo();
    let (integration_ref, commits) = setup_conflict_commits(dir.path());
    let integration_before = git_stdout(dir.path(), &["rev-parse", &integration_ref]);
    let integration_before = Oid::from_str(&integration_before).expect("oid");

    let outcome = replay_commits(
        &repo,
        &integration_ref,
        &commits,
        &ReplayOptions {
            continue_on_conflict: false,
        },
    )
    .expect("replay");

    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.entries.len(), 2);
    assert_eq!(outcome.entries[0].status, HoistCommitStatus::Conflict);
    assert_eq!(outcome.entries[1].status, HoistCommitStatus::Skipped);
    assert!(!tree_has_path(&repo, integration_before, "other.txt"));
}

#[test]
fn replay_commits_continues_on_conflict() {
    let (dir, repo) = init_repo();
    let (integration_ref, commits) = setup_conflict_commits(dir.path());

    let outcome = replay_commits(
        &repo,
        &integration_ref,
        &commits,
        &ReplayOptions {
            continue_on_conflict: true,
        },
    )
    .expect("replay");

    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.entries.len(), 2);
    assert_eq!(outcome.entries[0].status, HoistCommitStatus::Conflict);
    assert_eq!(outcome.entries[1].status, HoistCommitStatus::Applied);
    assert!(outcome.entries[1].applied_id.is_some());

    let applied = outcome.entries[1].applied_id.expect("applied");
    assert!(tree_has_path(&repo, applied, "other.txt"));
}
