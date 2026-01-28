//! Virtual merge infrastructure for conflict prediction.
//!
//! Performs an in-memory merge using libgit2 merge trees and returns
//! a conflict report without touching the working tree.

use git2::{Commit, ErrorCode, Index, IndexEntry, MergeOptions, Oid, Repository};
use serde::Serialize;

use crate::error::{Error, Result};

/// Result of a virtual merge simulation.
#[derive(Debug, Clone, Serialize)]
pub struct MergeSimulation {
    #[serde(serialize_with = "serialize_oid")]
    pub base: Oid,
    #[serde(serialize_with = "serialize_oid")]
    pub ours: Oid,
    #[serde(serialize_with = "serialize_oid")]
    pub theirs: Oid,
    pub conflicts: Vec<MergeConflict>,
}

fn serialize_oid<S: serde::Serializer>(oid: &Oid, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&oid.to_string())
}

/// A single merge conflict entry.
#[derive(Debug, Clone, Serialize)]
pub struct MergeConflict {
    pub path: String,
    pub kind: MergeConflictKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestor_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ours_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theirs_path: Option<String>,
}

/// Conflict category for high-level reporting.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeConflictKind {
    Content,
    AddAdd,
    ModifyDelete,
    Rename,
    Unknown,
}

/// Summarize conflicts for human-readable output.
pub fn summarize_conflicts(conflicts: &[MergeConflict]) -> Vec<String> {
    conflicts
        .iter()
        .map(|conflict| format!("{} ({})", conflict.path, conflict_kind_label(conflict.kind)))
        .collect()
}

/// Simulate a merge between two commit-ish references.
pub fn simulate_merge(
    repo: &Repository,
    ours_ref: &str,
    theirs_ref: &str,
    base_ref: Option<&str>,
) -> Result<MergeSimulation> {
    let ours_commit = resolve_commit(repo, ours_ref)?;
    let theirs_commit = resolve_commit(repo, theirs_ref)?;
    let base_commit = match base_ref {
        Some(spec) => resolve_commit(repo, spec)?,
        None => merge_base_commit(repo, ours_commit.id(), theirs_commit.id())?,
    };

    let base_tree = base_commit.tree()?;
    let ours_tree = ours_commit.tree()?;
    let theirs_tree = theirs_commit.tree()?;

    let mut options = MergeOptions::new();
    options.find_renames(true);

    let index = repo.merge_trees(&base_tree, &ours_tree, &theirs_tree, Some(&mut options))?;

    let conflicts = collect_conflicts(&index)?;

    Ok(MergeSimulation {
        base: base_commit.id(),
        ours: ours_commit.id(),
        theirs: theirs_commit.id(),
        conflicts,
    })
}

fn resolve_commit<'a>(repo: &'a Repository, spec: &str) -> Result<Commit<'a>> {
    let obj = repo.revparse_single(spec)?;
    obj.peel_to_commit().map_err(Error::Git)
}

fn merge_base_commit<'a>(repo: &'a Repository, ours: Oid, theirs: Oid) -> Result<Commit<'a>> {
    let base_oid = repo.merge_base(ours, theirs).map_err(|err| {
        if err.code() == ErrorCode::NotFound {
            Error::OperationFailed("no merge base found for inputs".to_string())
        } else {
            Error::Git(err)
        }
    })?;
    repo.find_commit(base_oid).map_err(Error::Git)
}

fn collect_conflicts(index: &Index) -> Result<Vec<MergeConflict>> {
    if !index.has_conflicts() {
        return Ok(Vec::new());
    }

    let mut conflicts = Vec::new();
    let iter = index.conflicts()?;
    for entry in iter {
        let entry = entry?;
        let ancestor_path = entry.ancestor.as_ref().map(entry_path);
        let ours_path = entry.our.as_ref().map(entry_path);
        let theirs_path = entry.their.as_ref().map(entry_path);

        let kind = classify_conflict(&ancestor_path, &ours_path, &theirs_path);
        let path = primary_path(&ancestor_path, &ours_path, &theirs_path);

        conflicts.push(MergeConflict {
            path,
            kind,
            ancestor_path,
            ours_path,
            theirs_path,
        });
    }

    conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(conflicts)
}

fn entry_path(entry: &IndexEntry) -> String {
    String::from_utf8_lossy(&entry.path).into_owned()
}

fn primary_path(
    ancestor: &Option<String>,
    ours: &Option<String>,
    theirs: &Option<String>,
) -> String {
    ours.clone()
        .or_else(|| theirs.clone())
        .or_else(|| ancestor.clone())
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn classify_conflict(
    ancestor: &Option<String>,
    ours: &Option<String>,
    theirs: &Option<String>,
) -> MergeConflictKind {
    let rename = paths_differ(ancestor, ours)
        || paths_differ(ancestor, theirs)
        || paths_differ(ours, theirs);

    match (ancestor.is_some(), ours.is_some(), theirs.is_some()) {
        (false, true, true) => {
            if rename {
                MergeConflictKind::Rename
            } else {
                MergeConflictKind::AddAdd
            }
        }
        (true, true, true) => {
            if rename {
                MergeConflictKind::Rename
            } else {
                MergeConflictKind::Content
            }
        }
        (true, true, false) | (true, false, true) => {
            if rename {
                MergeConflictKind::Rename
            } else {
                MergeConflictKind::ModifyDelete
            }
        }
        _ => MergeConflictKind::Unknown,
    }
}

fn conflict_kind_label(kind: MergeConflictKind) -> &'static str {
    match kind {
        MergeConflictKind::Content => "content",
        MergeConflictKind::AddAdd => "add/add",
        MergeConflictKind::ModifyDelete => "modify/delete",
        MergeConflictKind::Rename => "rename",
        MergeConflictKind::Unknown => "unknown",
    }
}

fn paths_differ(left: &Option<String>, right: &Option<String>) -> bool {
    match (left, right) {
        (Some(l), Some(r)) => l != r,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_conflicts_formats_lines() {
        let conflicts = vec![
            MergeConflict {
                path: "src/lib.rs".to_string(),
                kind: MergeConflictKind::Content,
                ancestor_path: None,
                ours_path: None,
                theirs_path: None,
            },
            MergeConflict {
                path: "README.md".to_string(),
                kind: MergeConflictKind::Rename,
                ancestor_path: None,
                ours_path: None,
                theirs_path: None,
            },
        ];

        let summary = summarize_conflicts(&conflicts);
        assert_eq!(
            summary,
            vec![
                "src/lib.rs (content)".to_string(),
                "README.md (rename)".to_string(),
            ]
        );
    }
}
