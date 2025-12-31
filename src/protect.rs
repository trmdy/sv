//! Protected path evaluation helpers.

use std::path::{Path, PathBuf};

use crate::config::{Config, ProtectRule};
use crate::error::{Error, Result};
use crate::storage::{ProtectOverride, Storage};

/// Status for a single protected path rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectRuleStatus {
    pub rule: ProtectRule,
    pub disabled: bool,
    pub matched_files: Vec<PathBuf>,
}

/// Aggregated protection status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectStatus {
    pub rules: Vec<ProtectRuleStatus>,
    pub disabled_patterns: Vec<String>,
}

/// Load per-workspace overrides from storage (if present).
pub fn load_override(storage: &Storage) -> Result<ProtectOverride> {
    let path = storage.protect_override_file();
    if !path.exists() {
        return Ok(ProtectOverride::default());
    }
    storage.read_json(&path)
}

/// Compute protection status for a set of staged files.
pub fn compute_status(
    config: &Config,
    override_data: Option<&ProtectOverride>,
    staged_files: &[PathBuf],
) -> Result<ProtectStatus> {
    let rules = config.protect.rules()?;
    let disabled_patterns = override_data
        .map(|data| data.disabled_patterns.clone())
        .unwrap_or_default();

    let statuses = rules
        .into_iter()
        .map(|rule| {
            let disabled = disabled_patterns.iter().any(|p| p == &rule.pattern);
            let matched_files = staged_files
                .iter()
                .filter_map(|path| match_pattern(&rule.pattern, path).transpose())
                .collect::<Result<Vec<_>>>()?;

            Ok(ProtectRuleStatus {
                rule,
                disabled,
                matched_files,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ProtectStatus {
        rules: statuses,
        disabled_patterns,
    })
}

fn match_pattern(pattern: &str, path: &Path) -> Result<Option<PathBuf>> {
    let matcher = glob::Pattern::new(pattern).map_err(|err| {
        Error::InvalidConfig(format!("invalid protect pattern '{pattern}': {err}"))
    })?;
    let normalized = normalize_path(path);
    if matcher.matches(&normalized) {
        Ok(Some(path.to_path_buf()))
    } else {
        Ok(None)
    }
}

fn normalize_path(path: &Path) -> String {
    let mut raw = path.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = raw.strip_prefix("./") {
        raw = stripped.to_string();
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_status_marks_disabled_patterns() {
        let mut config = Config::default();
        config.protect.mode = "guard".to_string();
        config.protect.paths = vec![
            crate::config::ProtectPath::Simple(".beads/**".to_string()),
            crate::config::ProtectPath::Simple("Cargo.lock".to_string()),
        ];

        let override_data = ProtectOverride {
            disabled_patterns: vec!["Cargo.lock".to_string()],
        };

        let staged = vec![
            PathBuf::from(".beads/issues.jsonl"),
            PathBuf::from("Cargo.lock"),
        ];

        let status = compute_status(&config, Some(&override_data), &staged).expect("status");
        assert_eq!(status.rules.len(), 2);
        assert!(status.rules[0].matched_files.len() == 1);
        assert!(status.rules[1].disabled);
    }

    #[test]
    fn normalize_path_strips_dot_prefix() {
        let path = PathBuf::from("./src/main.rs");
        assert_eq!(normalize_path(&path), "src/main.rs");
    }
}
