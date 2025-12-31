//! Risk analysis for overlapping workspace changes.
//!
//! Computes touched files per workspace (vs a base ref) and summarizes overlaps.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use git2::{DiffDelta, DiffOptions, Repository};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::lease::Lease;
use crate::storage::Storage;

/// Summary of touched files for a workspace.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceTouched {
    pub name: String,
    pub branch: String,
    pub files: Vec<String>,
}

/// Overlap summary for a specific path.
#[derive(Debug, Clone, Serialize)]
pub struct Overlap {
    pub path: String,
    pub workspaces: Vec<String>,
    pub severity: RiskSeverity,
    pub suggestions: Vec<Suggestion>,
}

/// Full risk report.
#[derive(Debug, Clone, Serialize)]
pub struct RiskReport {
    pub base_ref: String,
    pub workspaces: Vec<WorkspaceTouched>,
    pub overlaps: Vec<Overlap>,
}

/// Severity rating for an overlap.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Suggested follow-up action for an overlap.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub action: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// Compute a risk report for all registered workspaces.
pub fn compute_risk(repo: &Repository, base_ref: &str) -> Result<RiskReport> {
    let storage = load_storage(repo)?;
    let registry = storage.read_workspaces()?;
    let leases: Vec<Lease> = storage.read_jsonl(&storage.leases_file())?;
    let mut workspace_reports = Vec::new();

    for entry in registry.workspaces {
        let files = touched_files(repo, base_ref, &entry.branch)?;
        workspace_reports.push(WorkspaceTouched {
            name: entry.name,
            branch: entry.branch,
            files,
        });
    }

    let overlaps = compute_overlaps(&workspace_reports, &leases);

    Ok(RiskReport {
        base_ref: base_ref.to_string(),
        workspaces: workspace_reports,
        overlaps,
    })
}

fn compute_overlaps(workspaces: &[WorkspaceTouched], leases: &[Lease]) -> Vec<Overlap> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for workspace in workspaces {
        for path in &workspace.files {
            map.entry(path.clone())
                .or_default()
                .push(workspace.name.clone());
        }
    }

    let mut overlaps: Vec<Overlap> = map
        .into_iter()
        .filter_map(|(path, owners)| {
            let mut unique: HashSet<String> = owners.into_iter().collect();
            if unique.len() < 2 {
                return None;
            }
            let mut workspaces: Vec<String> = unique.drain().collect();
            workspaces.sort();
            let matching = matching_leases(leases, &path);
            let severity = severity_for(workspaces.len(), &matching);
            let suggestions = suggestions_for(&path, &workspaces, severity);
            Some(Overlap {
                path,
                workspaces,
                severity,
                suggestions,
            })
        })
        .collect();

    overlaps.sort_by(|a, b| a.path.cmp(&b.path));
    overlaps
}

fn severity_for(overlap_count: usize, leases: &[&Lease]) -> RiskSeverity {
    let overlap_score = overlap_count.min(4) as i32;
    let strength_score = leases
        .iter()
        .map(|lease| strength_weight(lease))
        .max()
        .unwrap_or(0);
    let intent_score = leases
        .iter()
        .map(|lease| lease.intent.conflict_risk() as i32)
        .max()
        .unwrap_or(0);

    let score = overlap_score + strength_score + intent_score;
    match score {
        0..=4 => RiskSeverity::Low,
        5..=7 => RiskSeverity::Medium,
        8..=10 => RiskSeverity::High,
        _ => RiskSeverity::Critical,
    }
}

fn suggestions_for(path: &str, workspaces: &[String], severity: RiskSeverity) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    suggestions.push(Suggestion {
        action: "take_lease".to_string(),
        reason: "Declare intent on the overlapping path to reduce duplicate work.".to_string(),
        command: Some(format!("sv take {path} --strength cooperative")),
    });

    suggestions.push(Suggestion {
        action: "inspect_leases".to_string(),
        reason: "See who currently holds overlapping leases and coordinate.".to_string(),
        command: Some(format!("sv lease who {path}")),
    });

    suggestions.push(Suggestion {
        action: "downgrade_lease".to_string(),
        reason: "If you hold a strong/exclusive lease, consider downgrading to cooperative."
            .to_string(),
        command: Some(format!(
            "sv release <lease-id> && sv take {path} --strength cooperative"
        )),
    });

    let onto_target = workspaces
        .get(0)
        .map(|name| name.as_str())
        .unwrap_or("<workspace>");
    suggestions.push(Suggestion {
        action: "rebase_onto".to_string(),
        reason: "Rebase onto an overlapping workspace to resolve conflicts earlier.".to_string(),
        command: Some(format!("sv onto {onto_target}")),
    });

    if matches!(severity, RiskSeverity::High | RiskSeverity::Critical) {
        suggestions.push(Suggestion {
            action: "pick_another_task".to_string(),
            reason: "High overlap risk; consider switching tasks.".to_string(),
            command: Some("bd ready --json".to_string()),
        });
    }

    suggestions
}

fn touched_files(repo: &Repository, base_ref: &str, branch_ref: &str) -> Result<Vec<String>> {
    let base_commit = repo
        .revparse_single(base_ref)?
        .peel_to_commit()?;
    let branch_commit = repo
        .revparse_single(branch_ref)?
        .peel_to_commit()?;

    let base_tree = base_commit.tree()?;
    let branch_tree = branch_commit.tree()?;

    let mut opts = DiffOptions::new();
    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&branch_tree), Some(&mut opts))?;

    let mut files = HashSet::new();
    diff.foreach(
        &mut |delta: DiffDelta<'_>, _| {
            if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                files.insert(path.to_string_lossy().to_string());
            }
            true
        },
        None,
        None,
        None,
    )?;

    let mut list: Vec<String> = files.into_iter().collect();
    list.sort();
    Ok(list)
}

fn load_storage(repo: &Repository) -> Result<Storage> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| Error::NotARepo(PathBuf::from(".")))?
        .to_path_buf();
    let git_dir = resolve_common_dir(repo)?;
    Ok(Storage::new(workdir.clone(), git_dir, workdir))
}

fn resolve_common_dir(repository: &Repository) -> Result<PathBuf> {
    let git_dir = repository.path();
    let commondir_path = git_dir.join("commondir");
    if !commondir_path.exists() {
        return Ok(git_dir.to_path_buf());
    }

    let content = std::fs::read_to_string(&commondir_path)?;
    let rel = content.trim();
    if rel.is_empty() {
        return Err(Error::OperationFailed(format!(
            "commondir file is empty: {}",
            commondir_path.display()
        )));
    }

    Ok(git_dir.join(rel))
}

fn matching_leases<'a>(leases: &'a [Lease], path: &str) -> Vec<&'a Lease> {
    leases
        .iter()
        .filter(|lease| lease.is_active() && lease.matches_path(path))
        .collect()
}

fn strength_weight(lease: &Lease) -> i32 {
    match lease.strength {
        crate::lease::LeaseStrength::Observe => 0,
        crate::lease::LeaseStrength::Cooperative => 1,
        crate::lease::LeaseStrength::Strong => 3,
        crate::lease::LeaseStrength::Exclusive => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lease::{LeaseBuilder, LeaseIntent, LeaseStrength};

    #[test]
    fn severity_scores_account_for_strength_and_intent() {
        let overlap_count = 2;
        let low = severity_for(overlap_count, &[]);
        assert!(matches!(low, RiskSeverity::Low));

        let strong = LeaseBuilder::new("src/lib.rs")
            .strength(LeaseStrength::Strong)
            .intent(LeaseIntent::Docs)
            .note("x")
            .build()
            .unwrap();
        let high = severity_for(overlap_count, &[&strong]);
        assert!(matches!(high, RiskSeverity::High));

        let exclusive = LeaseBuilder::new("src/main.rs")
            .strength(LeaseStrength::Exclusive)
            .intent(LeaseIntent::Rename)
            .note("x")
            .build()
            .unwrap();
        let critical = severity_for(overlap_count, &[&exclusive]);
        assert!(matches!(critical, RiskSeverity::Critical));
    }

    #[test]
    fn severity_increases_with_overlap_count() {
        let cooperative = LeaseBuilder::new("src/lib.rs")
            .strength(LeaseStrength::Cooperative)
            .intent(LeaseIntent::Feature)
            .build()
            .unwrap();

        let low = severity_for(2, &[&cooperative]);
        let medium = severity_for(3, &[&cooperative]);
        assert!(matches!(low, RiskSeverity::Low | RiskSeverity::Medium));
        assert!(matches!(medium, RiskSeverity::Medium | RiskSeverity::High));
    }

    #[test]
    fn suggestions_include_expected_actions() {
        let suggestions = suggestions_for("src/lib.rs", &["ws-a".to_string(), "ws-b".to_string()], RiskSeverity::Medium);
        let actions: Vec<&str> = suggestions.iter().map(|s| s.action.as_str()).collect();
        assert!(actions.contains(&"take_lease"));
        assert!(actions.contains(&"inspect_leases"));
        assert!(actions.contains(&"downgrade_lease"));
        assert!(actions.contains(&"rebase_onto"));
    }

    #[test]
    fn high_severity_includes_pick_another_task() {
        let suggestions = suggestions_for("src/lib.rs", &["ws-a".to_string(), "ws-b".to_string()], RiskSeverity::Critical);
        let actions: Vec<&str> = suggestions.iter().map(|s| s.action.as_str()).collect();
        assert!(actions.contains(&"pick_another_task"));
    }
}
