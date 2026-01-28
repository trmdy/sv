//! Change-Id trailer helpers.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use git2::{Commit, Oid, Repository};
use uuid::Uuid;

use crate::error::Result;

/// Generate a new Change-Id value.
pub fn generate_change_id() -> String {
    Uuid::new_v4().to_string()
}

/// Return the first Change-Id found in a commit message.
pub fn find_change_id(message: &str) -> Option<String> {
    for line in message.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Change-Id:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Ensure a Change-Id trailer is present in a message.
///
/// Returns the updated message and whether it was modified.
pub fn ensure_change_id(message: &str) -> (String, bool) {
    if find_change_id(message).is_some() {
        return (message.to_string(), false);
    }

    let change_id = generate_change_id();
    let updated = append_change_id(message, &change_id);
    (updated, true)
}

/// Ensure a Change-Id trailer exists in the commit message file.
///
/// Returns true if the file was modified.
pub fn ensure_change_id_file(path: &Path) -> Result<bool> {
    let contents = std::fs::read_to_string(path)?;
    let (updated, changed) = ensure_change_id(&contents);
    if changed {
        std::fs::write(path, updated)?;
    }
    Ok(changed)
}

/// Preferred selection policy for Change-Id deduplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefer {
    First,
    Last,
    Commit(Oid),
}

/// Commit OID paired with its patch id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPatchId {
    pub oid: Oid,
    pub patch_id: Oid,
}

/// Collapsed duplicate group with identical patch ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupedGroup {
    pub change_id: String,
    pub selected: Oid,
    pub dropped: Vec<Oid>,
    pub patch_id: Oid,
}

/// Diverged Change-Id group with conflicting patch ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupConflict {
    pub change_id: String,
    pub commits: Vec<CommitPatchId>,
}

/// Diverged Change-Id group resolved via a preference policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupWarning {
    pub change_id: String,
    pub commits: Vec<CommitPatchId>,
    pub selected: Oid,
}

/// Outcome for Change-Id deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupResult {
    pub selected: Vec<Oid>,
    pub deduped: Vec<DedupedGroup>,
    pub conflicts: Vec<DedupConflict>,
    pub warnings: Vec<DedupWarning>,
}

/// Deduplicate commits by Change-Id and patch-id.
///
/// - If Change-Id is missing, the commit is always kept.
/// - If a Change-Id group has identical patch-ids, keep one commit.
/// - If a Change-Id group diverges, return a conflict unless `prefer` is set.
pub fn dedup_commits_by_change_id(
    repo: &Repository,
    commits: &[Oid],
    prefer: Option<Prefer>,
) -> Result<DedupResult> {
    let mut groups: HashMap<String, Vec<Oid>> = HashMap::new();
    let mut has_change_id: HashMap<Oid, bool> = HashMap::new();

    for oid in commits {
        let commit = repo.find_commit(*oid)?;
        if let Some(change_id) = change_id_from_commit(&commit) {
            groups.entry(change_id).or_default().push(*oid);
            has_change_id.insert(*oid, true);
        } else {
            has_change_id.insert(*oid, false);
        }
    }

    let mut selected_set: HashSet<Oid> = HashSet::new();
    let mut deduped = Vec::new();
    let mut conflicts = Vec::new();
    let mut warnings = Vec::new();

    for (change_id, group) in groups {
        if group.len() == 1 {
            selected_set.insert(group[0]);
            continue;
        }

        let mut patch_ids = Vec::with_capacity(group.len());
        for oid in &group {
            let commit = repo.find_commit(*oid)?;
            let patch_id = patch_id_for_commit(repo, &commit)?;
            patch_ids.push(CommitPatchId {
                oid: *oid,
                patch_id,
            });
        }

        let first_patch = patch_ids[0].patch_id;
        let all_same = patch_ids.iter().all(|p| p.patch_id == first_patch);

        if all_same {
            let selected = select_preferred(prefer, &group);
            selected_set.insert(selected);
            let dropped = group
                .iter()
                .copied()
                .filter(|oid| *oid != selected)
                .collect();
            deduped.push(DedupedGroup {
                change_id,
                selected,
                dropped,
                patch_id: first_patch,
            });
        } else if let Some(prefer) = prefer {
            let selected = select_preferred(Some(prefer), &group);
            selected_set.insert(selected);
            warnings.push(DedupWarning {
                change_id,
                commits: patch_ids,
                selected,
            });
        } else {
            conflicts.push(DedupConflict {
                change_id,
                commits: patch_ids,
            });
        }
    }

    let mut selected = Vec::new();
    for oid in commits {
        if !has_change_id.get(oid).copied().unwrap_or(false) {
            selected.push(*oid);
            continue;
        }
        if selected_set.contains(oid) {
            selected.push(*oid);
        }
    }

    Ok(DedupResult {
        selected,
        deduped,
        conflicts,
        warnings,
    })
}

fn append_change_id(message: &str, change_id: &str) -> String {
    let trimmed = message.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return format!("Change-Id: {change_id}\n");
    }

    format!("{trimmed}\n\nChange-Id: {change_id}\n")
}

fn change_id_from_commit(commit: &Commit) -> Option<String> {
    let message = commit.message().unwrap_or_default();
    find_change_id(message)
}

fn patch_id_for_commit(repo: &Repository, commit: &Commit) -> Result<Oid> {
    let tree = commit.tree()?;
    let diff = if commit.parent_count() > 0 {
        let parent = commit.parent(0)?;
        let parent_tree = parent.tree()?;
        repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?
    } else {
        repo.diff_tree_to_tree(None, Some(&tree), None)?
    };
    Ok(diff.patchid(None)?)
}

fn select_preferred(prefer: Option<Prefer>, commits: &[Oid]) -> Oid {
    match prefer {
        Some(Prefer::Last) => *commits.last().expect("non-empty commit list"),
        Some(Prefer::Commit(oid)) if commits.contains(&oid) => oid,
        Some(Prefer::First) | Some(Prefer::Commit(_)) | None => commits[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn init_repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init repo");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Tester").expect("user.name");
        config
            .set_str("user.email", "tester@example.com")
            .expect("user.email");
        (dir, repo)
    }

    fn unique_signature_time() -> git2::Time {
        static COUNTER: AtomicI64 = AtomicI64::new(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let offset = 0;
        git2::Time::new(now + COUNTER.fetch_add(1, Ordering::SeqCst), offset)
    }

    fn commit_on_ref(
        repo: &Repository,
        refname: &str,
        parent: Option<Oid>,
        path: &str,
        content: &str,
        message: &str,
    ) -> Oid {
        let workdir = repo.workdir().expect("workdir");
        let file_path = workdir.join(path);
        std::fs::write(&file_path, content).expect("write file");

        let mut index = repo.index().expect("index");
        index.add_path(Path::new(path)).expect("add path to index");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("tree");
        let base_signature = repo.signature().expect("signature");
        let signature = git2::Signature::new(
            base_signature.name().unwrap_or("Tester"),
            base_signature.email().unwrap_or("tester@example.com"),
            &unique_signature_time(),
        )
        .expect("signature");

        let parents = parent
            .map(|oid| repo.find_commit(oid).expect("parent commit"))
            .into_iter()
            .collect::<Vec<_>>();
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        repo.commit(
            Some(refname),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit")
    }

    #[test]
    fn finds_existing_change_id() {
        let msg = "Fix: update config\n\nChange-Id: 1234\n";
        assert_eq!(find_change_id(msg), Some("1234".to_string()));
    }

    #[test]
    fn ensure_change_id_adds_trailer() {
        let msg = "Fix: update config";
        let (updated, changed) = ensure_change_id(msg);
        assert!(changed);
        assert!(updated.contains("\n\nChange-Id: "));
    }

    #[test]
    fn ensure_change_id_noop_when_present() {
        let msg = "Fix: update config\n\nChange-Id: abc";
        let (updated, changed) = ensure_change_id(msg);
        assert!(!changed);
        assert_eq!(updated, msg);
    }

    #[test]
    fn dedup_collapses_identical_patch_ids() {
        let (_dir, repo) = init_repo();
        let base = commit_on_ref(&repo, "HEAD", None, "file.txt", "base", "Base\n");
        let msg = "Change\n\nChange-Id: CID-1";
        let commit_a = commit_on_ref(
            &repo,
            "refs/heads/branch-a",
            Some(base),
            "file.txt",
            "change",
            msg,
        );
        let commit_b = commit_on_ref(
            &repo,
            "refs/heads/branch-b",
            Some(base),
            "file.txt",
            "change",
            msg,
        );

        let result = dedup_commits_by_change_id(&repo, &[commit_a, commit_b], None).unwrap();
        assert!(result.conflicts.is_empty());
        assert!(result.warnings.is_empty());
        assert_eq!(result.selected, vec![commit_a]);
        assert_eq!(result.deduped.len(), 1);
        assert_eq!(result.deduped[0].dropped, vec![commit_b]);
    }

    #[test]
    fn dedup_diverged_requires_prefer() {
        let (_dir, repo) = init_repo();
        let base = commit_on_ref(&repo, "HEAD", None, "file.txt", "base", "Base\n");
        let msg = "Change\n\nChange-Id: CID-2";
        let commit_a = commit_on_ref(
            &repo,
            "refs/heads/branch-a",
            Some(base),
            "file.txt",
            "change-one",
            msg,
        );
        let commit_b = commit_on_ref(
            &repo,
            "refs/heads/branch-b",
            Some(base),
            "file.txt",
            "change-two",
            msg,
        );

        let result = dedup_commits_by_change_id(&repo, &[commit_a, commit_b], None).unwrap();
        assert_eq!(result.selected.len(), 0);
        assert_eq!(result.conflicts.len(), 1);

        let resolved =
            dedup_commits_by_change_id(&repo, &[commit_a, commit_b], Some(Prefer::Last)).unwrap();
        assert_eq!(resolved.selected, vec![commit_b]);
        assert_eq!(resolved.warnings.len(), 1);
        assert!(resolved.conflicts.is_empty());
    }
}
