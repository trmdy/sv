//! Hoist helpers for deduplicating Change-Ids.
//!
//! This module focuses on Change-Id grouping and patch-id comparisons.
//! Hoist command wiring is handled elsewhere.

use std::collections::{HashMap, HashSet};

use git2::{Index, MergeOptions, Oid, Repository, Sort};

use crate::change_id::find_change_id;
use crate::error::Result;
use crate::git;
use crate::storage::HoistCommitStatus;

/// Candidate commit selected for hoist ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoistCandidate {
    pub oid: Oid,
    pub workspace: String,
}

/// Ordering modes for hoist replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderMode {
    /// Stable sort by workspace name, preserving commit order per workspace.
    Workspace,
    /// Sort by commit time (oldest first), stable by original order.
    Time,
    /// Prioritize an explicit workspace order, appending remaining workspaces alphabetically.
    Explicit(Vec<String>),
}

impl Default for OrderMode {
    fn default() -> Self {
        OrderMode::Workspace
    }
}

/// Workspace reference for hoist selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub name: String,
    pub branch: String,
}

/// Commits selected from a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCommits {
    pub name: String,
    pub branch: String,
    pub commits: Vec<Oid>,
}

/// Deduplication options for Change-Ids.
#[derive(Debug, Clone, Default)]
pub struct DedupOptions {
    /// Preferred commit per Change-Id when duplicates diverge.
    pub prefer: HashMap<String, Oid>,
}

/// Diverged Change-Id conflict summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeIdConflict {
    pub change_id: String,
    pub commits: Vec<Oid>,
    pub patch_ids: Vec<String>,
}

/// Result of Change-Id deduplication.
#[derive(Debug, Clone, Default)]
pub struct DedupOutcome {
    pub selected: Vec<Oid>,
    pub conflicts: Vec<ChangeIdConflict>,
    pub warnings: Vec<String>,
}

/// Replay outcome entry for a single commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEntry {
    pub commit_id: Oid,
    pub applied_id: Option<Oid>,
    pub status: HoistCommitStatus,
    pub change_id: Option<String>,
    pub summary: Option<String>,
}

/// Replay conflict record for a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayConflict {
    pub commit_id: Oid,
    pub files: Vec<String>,
    pub message: Option<String>,
}

/// Result of replaying commits.
#[derive(Debug, Clone, Default)]
pub struct ReplayOutcome {
    pub entries: Vec<ReplayEntry>,
    pub conflicts: Vec<ReplayConflict>,
}

/// Options for replaying commits onto an integration branch.
#[derive(Debug, Clone, Default)]
pub struct ReplayOptions {
    pub continue_on_conflict: bool,
}

/// Order hoist candidates based on the configured mode.
pub fn order_candidates(
    repo: &Repository,
    candidates: &[HoistCandidate],
    mode: &OrderMode,
) -> Result<Vec<HoistCandidate>> {
    match mode {
        OrderMode::Workspace => Ok(order_by_workspace(candidates)),
        OrderMode::Explicit(order) => Ok(order_by_explicit(candidates, order)),
        OrderMode::Time => order_by_time(repo, candidates),
    }
}

/// Collect commits ahead of the base ref for each workspace.
pub fn collect_workspace_commits(
    repo: &Repository,
    base_ref: &str,
    workspaces: &[WorkspaceRef],
) -> Result<Vec<WorkspaceCommits>> {
    let mut results = Vec::with_capacity(workspaces.len());
    for workspace in workspaces {
        let commits = commits_ahead_of(repo, base_ref, &workspace.branch)?;
        results.push(WorkspaceCommits {
            name: workspace.name.clone(),
            branch: workspace.branch.clone(),
            commits,
        });
    }
    Ok(results)
}

/// Flatten workspace commits into hoist candidates while preserving order.
pub fn candidates_from_workspace_commits(items: &[WorkspaceCommits]) -> Vec<HoistCandidate> {
    let mut candidates = Vec::new();
    for item in items {
        for oid in &item.commits {
            candidates.push(HoistCandidate {
                oid: *oid,
                workspace: item.name.clone(),
            });
        }
    }
    candidates
}

/// Collect and order hoist candidates for the provided workspaces.
pub fn select_hoist_commits(
    repo: &Repository,
    base_ref: &str,
    workspaces: &[WorkspaceRef],
    mode: &OrderMode,
) -> Result<Vec<HoistCandidate>> {
    let workspace_commits = collect_workspace_commits(repo, base_ref, workspaces)?;
    let candidates = candidates_from_workspace_commits(&workspace_commits);
    order_candidates(repo, &candidates, mode)
}

/// Replay commits onto the integration ref, returning per-commit outcomes.
pub fn replay_commits(
    repo: &Repository,
    integration_ref: &str,
    commits: &[Oid],
    options: &ReplayOptions,
) -> Result<ReplayOutcome> {
    let refname = normalize_refname(integration_ref);
    let mut current = repo.revparse_single(integration_ref)?.peel_to_commit()?;

    let mut outcome = ReplayOutcome::default();
    for (idx, oid) in commits.iter().enumerate() {
        let commit = repo.find_commit(*oid)?;
        let message = commit.message().unwrap_or_default();
        let summary = commit_summary(message);
        let change_id = find_change_id(message);

        let mut merge_opts = MergeOptions::new();
        let mut index = repo.cherrypick_commit(&commit, &current, 0, Some(&mut merge_opts))?;

        if index.has_conflicts() {
            let files = conflict_paths(&index)?;
            outcome.conflicts.push(ReplayConflict {
                commit_id: *oid,
                files,
                message: Some("conflict applying commit".to_string()),
            });
            outcome.entries.push(ReplayEntry {
                commit_id: *oid,
                applied_id: None,
                status: HoistCommitStatus::Conflict,
                change_id,
                summary,
            });

            if !options.continue_on_conflict {
                for remaining in &commits[idx + 1..] {
                    let remaining_commit = repo.find_commit(*remaining)?;
                    let remaining_msg = remaining_commit.message().unwrap_or_default();
                    outcome.entries.push(ReplayEntry {
                        commit_id: *remaining,
                        applied_id: None,
                        status: HoistCommitStatus::Skipped,
                        change_id: find_change_id(remaining_msg),
                        summary: commit_summary(remaining_msg),
                    });
                }
                break;
            }
            continue;
        }

        let tree_id = index.write_tree_to(repo)?;
        let tree = repo.find_tree(tree_id)?;
        let author = commit.author();
        let committer = commit.committer();
        let new_oid = repo.commit(
            Some(&refname),
            &author,
            &committer,
            message,
            &tree,
            &[&current],
        )?;

        current = repo.find_commit(new_oid)?;
        outcome.entries.push(ReplayEntry {
            commit_id: *oid,
            applied_id: Some(new_oid),
            status: HoistCommitStatus::Applied,
            change_id,
            summary,
        });
    }

    Ok(outcome)
}

/// Deduplicate commits by Change-Id, collapsing identical patch-ids.
///
/// Commits without Change-Ids are preserved as-is.
pub fn dedupe_change_ids(
    repo: &Repository,
    commits: &[Oid],
    options: &DedupOptions,
) -> Result<DedupOutcome> {
    let mut selected = HashSet::new();
    let mut conflicts = Vec::new();
    let mut warnings = Vec::new();
    let mut groups: HashMap<String, Vec<Oid>> = HashMap::new();
    let mut order: HashMap<Oid, usize> = HashMap::new();

    for (idx, oid) in commits.iter().enumerate() {
        order.insert(*oid, idx);
    }

    for oid in commits {
        let message = git::get_commit_message(repo, *oid)?;
        if let Some(change_id) = find_change_id(&message) {
            groups.entry(change_id).or_default().push(*oid);
        } else {
            selected.insert(*oid);
        }
    }

    for (change_id, group) in groups {
        if group.len() == 1 {
            selected.insert(group[0]);
            continue;
        }

        let mut by_patch: HashMap<String, Vec<Oid>> = HashMap::new();
        for oid in &group {
            let patch_id = git::patch_id(repo, *oid)?;
            by_patch.entry(patch_id).or_default().push(*oid);
        }

        if by_patch.len() == 1 {
            if let Some(chosen) = earliest_by_order(&group, &order) {
                selected.insert(chosen);
            }
            continue;
        }

        let mut patch_ids: Vec<String> = by_patch.keys().cloned().collect();
        patch_ids.sort();

        let mut commits = group.clone();
        commits.sort_by_key(|oid| order.get(oid).copied().unwrap_or(usize::MAX));

        if let Some(preferred) = options.prefer.get(&change_id).copied() {
            if commits.contains(&preferred) {
                selected.insert(preferred);
                warnings.push(format!(
                    "Change-Id {} diverged; using preferred commit {}",
                    change_id, preferred
                ));
                continue;
            }
        }

        warnings.push(format!(
            "Change-Id {} diverged across {} commits",
            change_id,
            commits.len()
        ));
        conflicts.push(ChangeIdConflict {
            change_id,
            commits,
            patch_ids,
        });
    }

    let mut ordered = Vec::new();
    let mut emitted = HashSet::new();
    for oid in commits {
        if selected.contains(oid) && emitted.insert(*oid) {
            ordered.push(*oid);
        }
    }

    Ok(DedupOutcome {
        selected: ordered,
        conflicts,
        warnings,
    })
}

fn earliest_by_order(group: &[Oid], order: &HashMap<Oid, usize>) -> Option<Oid> {
    group
        .iter()
        .copied()
        .min_by_key(|oid| order.get(oid).copied().unwrap_or(usize::MAX))
}

fn commits_ahead_of(repo: &Repository, base_ref: &str, branch_ref: &str) -> Result<Vec<Oid>> {
    let base = repo.revparse_single(base_ref)?.id();
    let branch = repo.revparse_single(branch_ref)?.id();

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
    revwalk.push(branch)?;
    revwalk.hide(base)?;

    let mut commits = Vec::new();
    for oid in revwalk {
        commits.push(oid?);
    }
    Ok(commits)
}

fn conflict_paths(index: &Index) -> Result<Vec<String>> {
    if !index.has_conflicts() {
        return Ok(Vec::new());
    }

    let mut paths = HashSet::new();
    let conflicts = index.conflicts()?;
    for conflict in conflicts {
        let conflict = conflict?;
        for entry in [conflict.ancestor, conflict.our, conflict.their] {
            if let Some(entry) = entry {
                let path = String::from_utf8_lossy(&entry.path).into_owned();
                if !path.is_empty() {
                    paths.insert(path);
                }
            }
        }
    }

    let mut out: Vec<String> = paths.into_iter().collect();
    out.sort();
    Ok(out)
}

fn normalize_refname(refname: &str) -> String {
    if refname.starts_with("refs/") {
        refname.to_string()
    } else {
        format!("refs/heads/{refname}")
    }
}

fn commit_summary(message: &str) -> Option<String> {
    message
        .lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn order_by_workspace(candidates: &[HoistCandidate]) -> Vec<HoistCandidate> {
    let mut by_workspace: HashMap<String, Vec<HoistCandidate>> = HashMap::new();
    for candidate in candidates {
        by_workspace
            .entry(candidate.workspace.clone())
            .or_default()
            .push(candidate.clone());
    }

    let mut workspaces: Vec<String> = by_workspace.keys().cloned().collect();
    workspaces.sort();

    let mut ordered = Vec::with_capacity(candidates.len());
    for workspace in workspaces {
        if let Some(items) = by_workspace.remove(&workspace) {
            ordered.extend(items);
        }
    }
    ordered
}

fn order_by_explicit(candidates: &[HoistCandidate], order: &[String]) -> Vec<HoistCandidate> {
    let mut by_workspace: HashMap<String, Vec<HoistCandidate>> = HashMap::new();
    for candidate in candidates {
        by_workspace
            .entry(candidate.workspace.clone())
            .or_default()
            .push(candidate.clone());
    }

    let mut ordered = Vec::with_capacity(candidates.len());
    let mut seen = HashSet::new();

    for workspace in order {
        if seen.insert(workspace) {
            if let Some(items) = by_workspace.remove(workspace) {
                ordered.extend(items);
            }
        }
    }

    let mut remaining: Vec<String> = by_workspace.keys().cloned().collect();
    remaining.sort();
    for workspace in remaining {
        if let Some(items) = by_workspace.remove(&workspace) {
            ordered.extend(items);
        }
    }

    ordered
}

fn order_by_time(repo: &Repository, candidates: &[HoistCandidate]) -> Result<Vec<HoistCandidate>> {
    let mut indexed: Vec<(usize, i64, HoistCandidate)> = Vec::with_capacity(candidates.len());
    for (idx, candidate) in candidates.iter().enumerate() {
        let commit = repo.find_commit(candidate.oid)?;
        indexed.push((idx, commit.time().seconds(), candidate.clone()));
    }

    indexed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    Ok(indexed.into_iter().map(|(_, _, candidate)| candidate).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

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

    fn git_with_env(repo: &Path, args: &[&str], envs: &[(&str, &str)]) {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(repo);
        for (key, value) in envs {
            cmd.env(key, value);
        }
        let output = cmd.output().expect("run git");
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

    fn create_branch(repo: &Path, name: &str, target: &str) {
        git(repo, &["branch", name, target]);
    }

    fn init_test_repo() -> (TempDir, Repository, String) {
        let temp = TempDir::new().expect("tempdir");
        git(temp.path(), &["init"]);
        git(temp.path(), &["config", "user.email", "test@test.com"]);
        git(temp.path(), &["config", "user.name", "Test"]);

        std::fs::write(temp.path().join("README.md"), "# Test\n").unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-m", "Initial commit"]);

        let branch = git_stdout(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"]);
        let repo = Repository::open(temp.path()).expect("open repo");
        (temp, repo, branch)
    }

    fn commit_with_date(repo: &Path, path: &str, contents: &str, title: &str, date: &str) -> Oid {
        std::fs::write(repo.join(path), contents).expect("write file");
        git(repo, &["add", path]);
        git_with_env(
            repo,
            &["commit", "-m", title],
            &[("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)],
        );

        let repo = Repository::open(repo).expect("open repo");
        let head = repo.head().expect("head");
        head.target().expect("commit oid")
    }

    fn commit_with_change_id(
        repo: &Path,
        path: &str,
        contents: &str,
        title: &str,
        change_id: &str,
    ) -> Oid {
        std::fs::write(repo.join(path), contents).expect("write file");
        git(repo, &["add", path]);
        let message = format!("{title}\n\nChange-Id: {change_id}");
        git(repo, &["commit", "-m", &message]);

        let repo = Repository::open(repo).expect("open repo");
        let head = repo.head().expect("head");
        head.target().expect("commit oid")
    }

    fn commit_simple(repo: &Path, path: &str, contents: &str, title: &str) -> Oid {
        std::fs::write(repo.join(path), contents).expect("write file");
        git(repo, &["add", path]);
        git(repo, &["commit", "-m", title]);

        let repo = Repository::open(repo).expect("open repo");
        let head = repo.head().expect("head");
        head.target().expect("commit oid")
    }

    #[test]
    fn dedupe_collapses_identical_change_id() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "branch-a"]);
        let commit_a = commit_with_change_id(
            temp.path(),
            "file.txt",
            "hello\n",
            "Add file",
            "change-1",
        );

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "branch-b"]);
        let commit_b = commit_with_change_id(
            temp.path(),
            "file.txt",
            "hello\n",
            "Add file",
            "change-1",
        );

        let outcome =
            dedupe_change_ids(&repo, &[commit_a, commit_b], &DedupOptions::default()).unwrap();

        assert!(outcome.conflicts.is_empty());
        assert_eq!(outcome.selected, vec![commit_a]);
    }

    #[test]
    fn dedupe_reports_diverged_change_id() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "branch-a"]);
        let commit_a = commit_with_change_id(
            temp.path(),
            "file.txt",
            "hello\n",
            "Add file",
            "change-2",
        );

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "branch-b"]);
        let commit_b = commit_with_change_id(
            temp.path(),
            "file.txt",
            "different\n",
            "Add file",
            "change-2",
        );

        let outcome =
            dedupe_change_ids(&repo, &[commit_a, commit_b], &DedupOptions::default()).unwrap();

        assert_eq!(outcome.conflicts.len(), 1);
        assert!(outcome.selected.is_empty());
    }

    #[test]
    fn dedupe_prefers_selected_commit() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "branch-a"]);
        let commit_a = commit_with_change_id(
            temp.path(),
            "file.txt",
            "hello\n",
            "Add file",
            "change-3",
        );

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "branch-b"]);
        let commit_b = commit_with_change_id(
            temp.path(),
            "file.txt",
            "different\n",
            "Add file",
            "change-3",
        );

        let mut options = DedupOptions::default();
        options.prefer.insert("change-3".to_string(), commit_b);

        let outcome = dedupe_change_ids(&repo, &[commit_a, commit_b], &options).unwrap();

        assert!(outcome.conflicts.is_empty());
        assert_eq!(outcome.selected, vec![commit_b]);
    }

    #[test]
    fn order_by_workspace_preserves_commit_order() {
        let candidates = vec![
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000001").unwrap(),
                workspace: "bravo".to_string(),
            },
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000002").unwrap(),
                workspace: "alpha".to_string(),
            },
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000003").unwrap(),
                workspace: "bravo".to_string(),
            },
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000004").unwrap(),
                workspace: "alpha".to_string(),
            },
        ];

        let ordered = order_by_workspace(&candidates);
        let ordered_oids: Vec<Oid> = ordered.into_iter().map(|c| c.oid).collect();
        assert_eq!(
            ordered_oids,
            vec![
                Oid::from_str("0000000000000000000000000000000000000002").unwrap(),
                Oid::from_str("0000000000000000000000000000000000000004").unwrap(),
                Oid::from_str("0000000000000000000000000000000000000001").unwrap(),
                Oid::from_str("0000000000000000000000000000000000000003").unwrap(),
            ]
        );
    }

    #[test]
    fn order_by_explicit_prioritizes_list() {
        let candidates = vec![
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000011").unwrap(),
                workspace: "bravo".to_string(),
            },
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000012").unwrap(),
                workspace: "alpha".to_string(),
            },
            HoistCandidate {
                oid: Oid::from_str("0000000000000000000000000000000000000013").unwrap(),
                workspace: "bravo".to_string(),
            },
        ];

        let ordered = order_by_explicit(&candidates, &["bravo".to_string()]);
        let ordered_workspaces: Vec<String> =
            ordered.into_iter().map(|c| c.workspace).collect();
        assert_eq!(
            ordered_workspaces,
            vec!["bravo".to_string(), "bravo".to_string(), "alpha".to_string()]
        );
    }

    #[test]
    fn order_by_time_uses_commit_timestamp() {
        let (temp, repo, _base_branch) = init_test_repo();

        let older = commit_with_date(
            temp.path(),
            "older.txt",
            "old",
            "Older commit",
            "2000-01-01T00:00:00Z",
        );
        let newer = commit_with_date(
            temp.path(),
            "newer.txt",
            "new",
            "Newer commit",
            "2000-01-02T00:00:00Z",
        );

        let candidates = vec![
            HoistCandidate {
                oid: newer,
                workspace: "alpha".to_string(),
            },
            HoistCandidate {
                oid: older,
                workspace: "alpha".to_string(),
            },
        ];

        let ordered = order_candidates(&repo, &candidates, &OrderMode::Time).unwrap();
        let ordered_oids: Vec<Oid> = ordered.into_iter().map(|c| c.oid).collect();
        assert_eq!(ordered_oids, vec![older, newer]);
    }

    #[test]
    fn collect_workspace_commits_lists_ahead_commits() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "alpha"]);
        let alpha_a = commit_simple(temp.path(), "alpha.txt", "a1", "alpha-1");
        let alpha_b = commit_simple(temp.path(), "alpha.txt", "a2", "alpha-2");

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "bravo"]);
        let bravo_a = commit_simple(temp.path(), "bravo.txt", "b1", "bravo-1");

        let workspaces = vec![
            WorkspaceRef {
                name: "alpha".to_string(),
                branch: "alpha".to_string(),
            },
            WorkspaceRef {
                name: "bravo".to_string(),
                branch: "bravo".to_string(),
            },
        ];

        let results =
            collect_workspace_commits(&repo, &base_branch, &workspaces).expect("collect");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].commits, vec![alpha_a, alpha_b]);
        assert_eq!(results[1].commits, vec![bravo_a]);
    }

    #[test]
    fn select_hoist_commits_orders_by_workspace() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "bravo"]);
        let bravo = commit_simple(temp.path(), "bravo.txt", "b1", "bravo-1");

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "alpha"]);
        let alpha = commit_simple(temp.path(), "alpha.txt", "a1", "alpha-1");

        let workspaces = vec![
            WorkspaceRef {
                name: "bravo".to_string(),
                branch: "bravo".to_string(),
            },
            WorkspaceRef {
                name: "alpha".to_string(),
                branch: "alpha".to_string(),
            },
        ];

        let ordered =
            select_hoist_commits(&repo, &base_branch, &workspaces, &OrderMode::Workspace)
                .expect("select");
        let ordered_oids: Vec<Oid> = ordered.into_iter().map(|c| c.oid).collect();
        assert_eq!(ordered_oids, vec![alpha, bravo]);
    }

    #[test]
    fn select_hoist_commits_respects_explicit_order() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "bravo"]);
        let bravo = commit_simple(temp.path(), "bravo.txt", "b1", "bravo-1");

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "alpha"]);
        let alpha = commit_simple(temp.path(), "alpha.txt", "a1", "alpha-1");

        let workspaces = vec![
            WorkspaceRef {
                name: "alpha".to_string(),
                branch: "alpha".to_string(),
            },
            WorkspaceRef {
                name: "bravo".to_string(),
                branch: "bravo".to_string(),
            },
        ];

        let ordered = select_hoist_commits(
            &repo,
            &base_branch,
            &workspaces,
            &OrderMode::Explicit(vec!["bravo".to_string()]),
        )
        .expect("select");
        let ordered_oids: Vec<Oid> = ordered.into_iter().map(|c| c.oid).collect();
        assert_eq!(ordered_oids, vec![bravo, alpha]);
    }

    #[test]
    fn replay_commits_applies_commit() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "alpha"]);
        let alpha = commit_simple(temp.path(), "alpha.txt", "alpha", "alpha-1");

        git(temp.path(), &["checkout", &base_branch]);
        create_branch(temp.path(), "sv/hoist/main", &base_branch);

        let outcome =
            replay_commits(&repo, "sv/hoist/main", &[alpha], &ReplayOptions::default()).unwrap();

        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].status, HoistCommitStatus::Applied);
        let applied = outcome.entries[0].applied_id.expect("applied id");

        let integration_tip = repo.revparse_single("sv/hoist/main").unwrap().id();
        assert_eq!(integration_tip, applied);
    }

    #[test]
    fn replay_commits_reports_conflict_and_stops() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "alpha"]);
        let alpha = commit_simple(temp.path(), "README.md", "alpha", "alpha-1");

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "bravo"]);
        let bravo = commit_simple(temp.path(), "README.md", "bravo", "bravo-1");

        create_branch(temp.path(), "sv/hoist/main", &base_branch);

        let outcome = replay_commits(
            &repo,
            "sv/hoist/main",
            &[alpha, bravo],
            &ReplayOptions {
                continue_on_conflict: false,
            },
        )
        .unwrap();

        assert_eq!(outcome.conflicts.len(), 1);
        assert_eq!(outcome.entries.len(), 2);
        assert_eq!(outcome.entries[0].status, HoistCommitStatus::Applied);
        assert_eq!(outcome.entries[1].status, HoistCommitStatus::Conflict);
    }

    #[test]
    fn replay_commits_continues_after_conflict() {
        let (temp, repo, base_branch) = init_test_repo();

        git(temp.path(), &["checkout", "-b", "alpha"]);
        let alpha = commit_simple(temp.path(), "README.md", "alpha", "alpha-1");

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "bravo"]);
        let bravo = commit_simple(temp.path(), "README.md", "bravo", "bravo-1");

        git(temp.path(), &["checkout", &base_branch]);
        git(temp.path(), &["checkout", "-b", "charlie"]);
        let charlie = commit_simple(temp.path(), "notes.txt", "notes", "charlie-1");

        create_branch(temp.path(), "sv/hoist/main", &base_branch);

        let outcome = replay_commits(
            &repo,
            "sv/hoist/main",
            &[alpha, bravo, charlie],
            &ReplayOptions {
                continue_on_conflict: true,
            },
        )
        .unwrap();

        assert_eq!(outcome.conflicts.len(), 1);
        assert_eq!(outcome.entries.len(), 3);
        assert_eq!(outcome.entries[0].status, HoistCommitStatus::Applied);
        assert_eq!(outcome.entries[1].status, HoistCommitStatus::Conflict);
        assert_eq!(outcome.entries[2].status, HoistCommitStatus::Applied);

        let integration_tip = repo.revparse_single("sv/hoist/main").unwrap().id();
        assert_eq!(integration_tip, outcome.entries[2].applied_id.unwrap());
    }
}
