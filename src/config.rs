//! Configuration loading and management
//!
//! Handles parsing of `.sv.toml` configuration files.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default base branch for new workspaces
    #[serde(default = "default_base")]
    pub base: String,

    /// Actor configuration
    #[serde(default)]
    pub actor: ActorConfig,

    /// Lease configuration
    #[serde(default)]
    pub leases: LeaseConfig,

    /// Protected paths configuration
    #[serde(default)]
    pub protect: ProtectConfig,

    /// Tasks configuration
    #[serde(default)]
    pub tasks: TasksConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base: default_base(),
            actor: ActorConfig::default(),
            leases: LeaseConfig::default(),
            protect: ProtectConfig::default(),
            tasks: TasksConfig::default(),
        }
    }
}

fn default_base() -> String {
    "main".to_string()
}

/// Actor-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorConfig {
    /// Default actor name when none specified
    #[serde(default = "default_actor")]
    pub default: String,
}

fn default_actor() -> String {
    "unknown".to_string()
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            default: default_actor(),
        }
    }
}

/// Lease-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseConfig {
    /// Default lease strength
    #[serde(default = "default_strength")]
    pub default_strength: String,

    /// Default lease intent
    #[serde(default = "default_intent")]
    pub default_intent: String,

    /// Default TTL
    #[serde(default = "default_ttl")]
    pub default_ttl: String,

    /// Grace period before removing expired leases
    #[serde(default = "default_expiration_grace")]
    pub expiration_grace: String,

    /// Require a note for strong/exclusive leases
    #[serde(default = "default_require_note")]
    pub require_note: bool,

    /// Compatibility rules
    #[serde(default)]
    pub compat: LeaseCompatConfig,
}

fn default_strength() -> String {
    "cooperative".to_string()
}

fn default_intent() -> String {
    "other".to_string()
}

fn default_ttl() -> String {
    "2h".to_string()
}

fn default_expiration_grace() -> String {
    "0s".to_string()
}

fn default_require_note() -> bool {
    true
}

impl Default for LeaseConfig {
    fn default() -> Self {
        Self {
            default_strength: default_strength(),
            default_intent: default_intent(),
            default_ttl: default_ttl(),
            expiration_grace: default_expiration_grace(),
            require_note: default_require_note(),
            compat: LeaseCompatConfig::default(),
        }
    }
}

/// Lease compatibility configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseCompatConfig {
    /// Allow cooperative leases to overlap
    #[serde(default = "default_true")]
    pub allow_overlap_cooperative: bool,

    /// Require --allow-overlap flag for strong leases
    #[serde(default = "default_true")]
    pub require_flag_for_strong_overlap: bool,
}

fn default_true() -> bool {
    true
}

impl Default for LeaseCompatConfig {
    fn default() -> Self {
        Self {
            allow_overlap_cooperative: true,
            require_flag_for_strong_overlap: true,
        }
    }
}

/// Protected paths configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectConfig {
    /// Default protection mode
    #[serde(default = "default_protect_mode")]
    pub mode: String,

    /// Protected path patterns
    #[serde(default)]
    pub paths: Vec<ProtectPath>,
}

/// Protected path entry with optional per-path mode override.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProtectPath {
    /// Simple pattern that uses the default protect mode.
    Simple(String),
    /// Pattern with an explicit mode override.
    WithMode {
        #[serde(alias = "path")]
        pattern: String,
        mode: String,
    },
}

/// Normalized protect rule with an explicit mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectRule {
    pub pattern: String,
    pub mode: String,
}

fn default_protect_mode() -> String {
    "guard".to_string()
}

impl Default for ProtectConfig {
    fn default() -> Self {
        Self {
            mode: default_protect_mode(),
            paths: vec![],
        }
    }
}

/// Tasks configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksConfig {
    /// Repo-wide task ID prefix
    #[serde(default = "default_task_id_prefix")]
    pub id_prefix: String,

    /// Minimum task ID suffix length
    #[serde(default = "default_task_id_min_len")]
    pub id_min_len: usize,

    /// Allowed task statuses
    #[serde(default = "default_task_statuses")]
    pub statuses: Vec<String>,

    /// Default status for new tasks
    #[serde(default = "default_task_status")]
    pub default_status: String,

    /// Status used by `sv task start`
    #[serde(default = "default_task_in_progress_status")]
    pub in_progress_status: String,

    /// Statuses considered closed
    #[serde(default = "default_task_closed_statuses")]
    pub closed_statuses: Vec<String>,

    /// Compaction policy
    #[serde(default)]
    pub compaction: TasksCompactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksCompactionConfig {
    /// Enable auto-compaction during sync
    #[serde(default)]
    pub auto: bool,

    /// Auto-compact when log exceeds this size in MB
    #[serde(default = "default_task_compaction_max_log_mb")]
    pub max_log_mb: u64,

    /// Auto-compact tasks older than this duration (e.g., "180d")
    #[serde(default = "default_task_compaction_older_than")]
    pub older_than: String,
}

fn default_task_statuses() -> Vec<String> {
    vec!["open".to_string(), "in_progress".to_string(), "closed".to_string()]
}

fn default_task_id_prefix() -> String {
    "sv".to_string()
}

fn default_task_id_min_len() -> usize {
    3
}

fn default_task_status() -> String {
    "open".to_string()
}

fn default_task_in_progress_status() -> String {
    "in_progress".to_string()
}

fn default_task_closed_statuses() -> Vec<String> {
    vec!["closed".to_string()]
}

fn default_task_compaction_max_log_mb() -> u64 {
    200
}

fn default_task_compaction_older_than() -> String {
    "180d".to_string()
}

impl Default for TasksCompactionConfig {
    fn default() -> Self {
        Self {
            auto: false,
            max_log_mb: default_task_compaction_max_log_mb(),
            older_than: default_task_compaction_older_than(),
        }
    }
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self {
            id_prefix: default_task_id_prefix(),
            id_min_len: default_task_id_min_len(),
            statuses: default_task_statuses(),
            default_status: default_task_status(),
            in_progress_status: default_task_in_progress_status(),
            closed_statuses: default_task_closed_statuses(),
            compaction: TasksCompactionConfig::default(),
        }
    }
}

impl ProtectConfig {
    pub fn rules(&self) -> crate::error::Result<Vec<ProtectRule>> {
        self.validate()?;
        Ok(self
            .paths
            .iter()
            .map(|entry| match entry {
                ProtectPath::Simple(pattern) => ProtectRule {
                    pattern: pattern.to_string(),
                    mode: self.mode.clone(),
                },
                ProtectPath::WithMode { pattern, mode } => ProtectRule {
                    pattern: pattern.to_string(),
                    mode: mode.to_string(),
                },
            })
            .collect())
    }

    fn validate(&self) -> crate::error::Result<()> {
        validate_protect_mode(&self.mode, "protect.mode")?;
        for entry in &self.paths {
            match entry {
                ProtectPath::Simple(pattern) => {
                    validate_pattern(pattern, "protect.paths")?;
                }
                ProtectPath::WithMode { pattern, mode } => {
                    validate_pattern(pattern, "protect.paths")?;
                    validate_protect_mode(mode, "protect.paths.mode")?;
                }
            }
        }
        Ok(())
    }
}

fn validate_pattern(pattern: &str, field: &str) -> crate::error::Result<()> {
    if pattern.trim().is_empty() {
        return Err(crate::error::Error::InvalidConfig(format!(
            "{field}: pattern cannot be empty"
        )));
    }
    glob::Pattern::new(pattern).map_err(|err| {
        crate::error::Error::InvalidConfig(format!(
            "{field}: invalid glob pattern '{pattern}': {err}"
        ))
    })?;
    Ok(())
}

fn validate_protect_mode(mode: &str, field: &str) -> crate::error::Result<()> {
    match mode {
        "guard" | "readonly" | "warn" => Ok(()),
        _ => Err(crate::error::Error::InvalidConfig(format!(
            "{field}: invalid mode '{mode}' (expected guard|readonly|warn)"
        ))),
    }
}

impl Config {
    /// Load configuration from a `.sv.toml` file
    pub fn load(path: &PathBuf) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from repo root, or return defaults
    pub fn load_from_repo(repo_root: &PathBuf) -> Self {
        let config_path = repo_root.join(".sv.toml");
        if config_path.exists() {
            Self::load(&config_path).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Save configuration to a file
    pub fn save(&self, path: &PathBuf) -> crate::error::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn validate(&self) -> crate::error::Result<()> {
        self.protect.validate()?;
        self.tasks.validate()?;
        Ok(())
    }
}

impl TasksConfig {
    fn validate(&self) -> crate::error::Result<()> {
        let prefix = self.id_prefix.trim();
        if prefix.is_empty() {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.id_prefix cannot be empty".to_string(),
            ));
        }
        if !prefix.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.id_prefix must be alphanumeric".to_string(),
            ));
        }
        if self.id_min_len < 3 {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.id_min_len must be >= 3".to_string(),
            ));
        }
        if self.id_min_len > 16 {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.id_min_len must be <= 16".to_string(),
            ));
        }

        if self.statuses.is_empty() {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.statuses cannot be empty".to_string(),
            ));
        }

        let mut seen = std::collections::HashSet::new();
        for status in &self.statuses {
            let trimmed = status.trim();
            if trimmed.is_empty() {
                return Err(crate::error::Error::InvalidConfig(
                    "tasks.statuses cannot include empty entries".to_string(),
                ));
            }
            if !seen.insert(trimmed.to_string()) {
                return Err(crate::error::Error::InvalidConfig(format!(
                    "tasks.statuses has duplicate entry '{trimmed}'"
                )));
            }
        }

        if !seen.contains(self.default_status.trim()) {
            return Err(crate::error::Error::InvalidConfig(format!(
                "tasks.default_status '{}' not in tasks.statuses",
                self.default_status
            )));
        }

        if !seen.contains(self.in_progress_status.trim()) {
            return Err(crate::error::Error::InvalidConfig(format!(
                "tasks.in_progress_status '{}' not in tasks.statuses",
                self.in_progress_status
            )));
        }

        for status in &self.closed_statuses {
            let trimmed = status.trim();
            if trimmed.is_empty() {
                return Err(crate::error::Error::InvalidConfig(
                    "tasks.closed_statuses cannot include empty entries".to_string(),
                ));
            }
            if !seen.contains(trimmed) {
                return Err(crate::error::Error::InvalidConfig(format!(
                    "tasks.closed_statuses '{}' not in tasks.statuses",
                    trimmed
                )));
            }
        }

        if self.closed_statuses.is_empty() {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.closed_statuses cannot be empty".to_string(),
            ));
        }

        if self.compaction.max_log_mb == 0 {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.compaction.max_log_mb must be > 0".to_string(),
            ));
        }

        if self.compaction.older_than.trim().is_empty() {
            return Err(crate::error::Error::InvalidConfig(
                "tasks.compaction.older_than cannot be empty".to_string(),
            ));
        }

        crate::lease::parse_duration(&self.compaction.older_than)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn defaults_are_expected() {
        let cfg = Config::default();
        assert_eq!(cfg.base, "main");
        assert_eq!(cfg.actor.default, "unknown");
        assert_eq!(cfg.leases.default_strength, "cooperative");
        assert_eq!(cfg.leases.default_intent, "other");
        assert_eq!(cfg.leases.default_ttl, "2h");
        assert_eq!(cfg.leases.expiration_grace, "0s");
        assert!(cfg.leases.compat.allow_overlap_cooperative);
        assert!(cfg.leases.compat.require_flag_for_strong_overlap);
        assert_eq!(cfg.protect.mode, "guard");
        assert!(cfg.protect.paths.is_empty());
        assert_eq!(
            cfg.tasks.statuses,
            vec!["open".to_string(), "in_progress".to_string(), "closed".to_string()]
        );
        assert_eq!(cfg.tasks.id_prefix, "sv");
        assert_eq!(cfg.tasks.id_min_len, 3);
        assert_eq!(cfg.tasks.default_status, "open");
        assert_eq!(cfg.tasks.in_progress_status, "in_progress");
        assert_eq!(cfg.tasks.closed_statuses, vec!["closed".to_string()]);
        assert!(!cfg.tasks.compaction.auto);
        assert_eq!(cfg.tasks.compaction.max_log_mb, 200);
        assert_eq!(cfg.tasks.compaction.older_than, "180d");
    }

    #[test]
    fn load_parses_overrides() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".sv.toml");
        let content = r#"
base = "dev"

[actor]
default = "alice"

[leases]
default_strength = "exclusive"
default_intent = "bugfix"
default_ttl = "30m"
expiration_grace = "10m"

[leases.compat]
allow_overlap_cooperative = false
require_flag_for_strong_overlap = false

[protect]
mode = "warn"
paths = [".beads/**", { pattern = "Cargo.lock", mode = "readonly" }]

[tasks]
id_prefix = "proj"
id_min_len = 4
statuses = ["open", "review", "closed"]
default_status = "open"
in_progress_status = "review"
closed_statuses = ["closed"]

[tasks.compaction]
auto = true
max_log_mb = 50
older_than = "90d"
"#;
        fs::write(&path, content.trim()).expect("write config");

        let cfg = Config::load(&path).expect("load config");
        assert_eq!(cfg.base, "dev");
        assert_eq!(cfg.actor.default, "alice");
        assert_eq!(cfg.leases.default_strength, "exclusive");
        assert_eq!(cfg.leases.default_intent, "bugfix");
        assert_eq!(cfg.leases.default_ttl, "30m");
        assert_eq!(cfg.leases.expiration_grace, "10m");
        assert!(!cfg.leases.compat.allow_overlap_cooperative);
        assert!(!cfg.leases.compat.require_flag_for_strong_overlap);
        assert_eq!(cfg.protect.mode, "warn");
        assert_eq!(
            cfg.protect.rules().expect("rules"),
            vec![
                ProtectRule {
                    pattern: ".beads/**".to_string(),
                    mode: "warn".to_string(),
                },
                ProtectRule {
                    pattern: "Cargo.lock".to_string(),
                    mode: "readonly".to_string(),
                },
            ]
        );
        assert_eq!(
            cfg.tasks.statuses,
            vec!["open".to_string(), "review".to_string(), "closed".to_string()]
        );
        assert_eq!(cfg.tasks.id_prefix, "proj");
        assert_eq!(cfg.tasks.id_min_len, 4);
        assert_eq!(cfg.tasks.default_status, "open");
        assert_eq!(cfg.tasks.in_progress_status, "review");
        assert_eq!(cfg.tasks.closed_statuses, vec!["closed".to_string()]);
        assert!(cfg.tasks.compaction.auto);
        assert_eq!(cfg.tasks.compaction.max_log_mb, 50);
        assert_eq!(cfg.tasks.compaction.older_than, "90d");
    }

    #[test]
    fn invalid_protect_mode_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".sv.toml");
        let content = r#"
[protect]
mode = "nope"
paths = [".beads/**"]
"#;
        fs::write(&path, content.trim()).expect("write config");

        let err = Config::load(&path).expect_err("invalid config");
        match err {
            crate::error::Error::InvalidConfig(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn invalid_task_config_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".sv.toml");
        let content = r#"
[tasks]
id_prefix = ""
statuses = ["open", "closed"]
default_status = "missing"
"#;
        fs::write(&path, content.trim()).expect("write config");

        let err = Config::load(&path).expect_err("invalid config");
        match err {
            crate::error::Error::InvalidConfig(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_from_repo_defaults_when_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config::load_from_repo(&dir.path().to_path_buf());
        assert_eq!(cfg.base, "main");
    }

    #[test]
    fn load_from_repo_reads_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".sv.toml");
        fs::write(&path, "base = \"feature\"").expect("write config");

        let cfg = Config::load_from_repo(&dir.path().to_path_buf());
        assert_eq!(cfg.base, "feature");
    }

    #[test]
    fn save_writes_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.toml");
        let cfg = Config::default();
        cfg.save(&path).expect("save config");

        let written = fs::read_to_string(&path).expect("read config");
        assert!(written.contains("base = \"main\""));
    }
}
