//! Branch and ref operations using libgit2.

use git2::{BranchType, Oid, Repository};

use crate::error::{Error, Result};

/// Create a local branch from a ref (commit-ish).
pub fn create_branch_from_ref(
    repo: &Repository,
    name: &str,
    target: &str,
    force: bool,
) -> Result<()> {
    let obj = repo.revparse_single(target)?;
    let commit = obj.peel_to_commit()?;
    repo.branch(name, &commit, force)?;
    Ok(())
}

/// Delete a local branch by name.
pub fn delete_branch(repo: &Repository, name: &str) -> Result<()> {
    let mut branch = repo.find_branch(name, BranchType::Local)?;
    branch.delete()?;
    Ok(())
}

/// Resolve a ref or revspec to the target commit OID.
pub fn resolve_ref_oid(repo: &Repository, spec: &str) -> Result<Oid> {
    let obj = repo.revparse_single(spec)?;
    let commit = obj.peel_to_commit()?;
    Ok(commit.id())
}

/// Move a local branch reference to a new target commit.
pub fn move_branch_ref(repo: &Repository, name: &str, target: Oid) -> Result<()> {
    let refname = format!("refs/heads/{name}");
    let mut reference = repo.find_reference(&refname)?;
    reference.set_target(target, "sv move branch")?;
    Ok(())
}

/// List local branches, optionally filtered by a glob pattern.
pub fn list_branches(repo: &Repository, pattern: Option<&str>) -> Result<Vec<String>> {
    let matcher = if let Some(pattern) = pattern {
        Some(
            glob::Pattern::new(pattern).map_err(|err| {
                Error::InvalidArgument(format!("invalid branch pattern '{pattern}': {err}"))
            })?,
        )
    } else {
        None
    };

    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        if let Some(name) = branch.name()? {
            let matches = match &matcher {
                Some(glob) => glob.matches(name),
                None => true,
            };
            if matches {
                branches.push(name.to_string());
            }
        }
    }

    Ok(branches)
}
