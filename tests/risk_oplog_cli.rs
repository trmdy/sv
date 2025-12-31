mod support;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use chrono::Utc;
use git2::{Oid, Repository};
use serde_json::Value;

use support::TestRepo;
use sv::oplog::{OpLog, OpRecord};
use sv::storage::{Storage, WorkspaceEntry};

fn sv_cmd(repo: &TestRepo) -> Command {
    let mut cmd = Command::cargo_bin("sv").expect("binary");
    cmd.current_dir(repo.path());
    cmd
}

fn commit_on_ref(
    repo: &Repository,
    refname: &str,
    parent: Option<Oid>,
    path: &str,
    content: &str,
    message: &str,
) -> Result<Oid, Box<dyn std::error::Error>> {
    let workdir = repo.workdir().ok_or("repo has no workdir")?;
    let file_path = workdir.join(path);
    if let Some(parent_dir) = file_path.parent() {
        fs::create_dir_all(parent_dir)?;
    }
    fs::write(&file_path, content)?;

    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo.signature()?;

    let oid = match parent {
        Some(parent_oid) => {
            let parent_commit = repo.find_commit(parent_oid)?;
            repo.commit(
                Some(refname),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent_commit],
            )?
        }
        None => repo.commit(
            Some(refname),
            &signature,
            &signature,
            message,
            &tree,
            &[],
        )?,
    };

    Ok(oid)
}

#[test]
fn op_log_reports_records_json() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let log = OpLog::for_storage(&storage);
    let mut record = OpRecord::new("sv ws new ws-a", Some("agent1".to_string()));
    record.affected_workspaces.push("ws-a".to_string());
    log.append(&record)?;

    let output = sv_cmd(&repo)
        .args(["op", "log", "--limit", "1", "--json"])
        .output()?;
    assert!(output.status.success());

    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["total"].as_u64(), Some(1));
    assert_eq!(
        report["records"][0]["command"].as_str(),
        Some("sv ws new ws-a")
    );
    assert_eq!(report["records"][0]["actor"].as_str(), Some("agent1"));

    Ok(())
}

#[test]
fn risk_reports_overlaps_for_shared_paths() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.commit_file("README.md", "base\n", "base commit")?;

    let git_repo = repo.repo();
    let base = git_repo
        .head()?
        .target()
        .ok_or("missing base head")?;

    let branch_a = "refs/heads/sv/ws/ws-a";
    let branch_b = "refs/heads/sv/ws/ws-b";
    commit_on_ref(
        git_repo,
        branch_a,
        Some(base),
        "README.md",
        "change a\n",
        "ws-a change",
    )?;
    commit_on_ref(
        git_repo,
        branch_b,
        Some(base),
        "README.md",
        "change b\n",
        "ws-b change",
    )?;

    let storage = Storage::for_repo(repo.path().to_path_buf());
    let worktree_a = repo.path().join("worktrees/ws-a");
    let worktree_b = repo.path().join("worktrees/ws-b");
    fs::create_dir_all(&worktree_a)?;
    fs::create_dir_all(&worktree_b)?;

    storage.add_workspace(WorkspaceEntry::new(
        "ws-a".to_string(),
        worktree_a,
        "sv/ws/ws-a".to_string(),
        "HEAD".to_string(),
        None,
        Utc::now().to_rfc3339(),
        None,
    ))?;
    storage.add_workspace(WorkspaceEntry::new(
        "ws-b".to_string(),
        worktree_b,
        "sv/ws/ws-b".to_string(),
        "HEAD".to_string(),
        None,
        Utc::now().to_rfc3339(),
        None,
    ))?;

    let output = sv_cmd(&repo)
        .args(["risk", "--base", "HEAD", "--json"])
        .output()?;
    assert!(output.status.success());

    let report: Value = serde_json::from_slice(&output.stdout)?;
    let overlaps = report["overlaps"]
        .as_array()
        .ok_or("overlaps is not array")?;
    assert!(!overlaps.is_empty());

    let overlap = overlaps
        .iter()
        .find(|item| item["path"].as_str() == Some("README.md"))
        .ok_or("README.md overlap missing")?;
    let workspaces = overlap["workspaces"]
        .as_array()
        .ok_or("workspaces not array")?;
    let names: Vec<&str> = workspaces
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(names.contains(&"ws-a"));
    assert!(names.contains(&"ws-b"));

    Ok(())
}
