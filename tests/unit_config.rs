use std::fs;

use sv::config::{Config, ProtectPath};

#[test]
fn config_defaults_when_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Config::load_from_repo(&dir.path().to_path_buf());

    assert_eq!(config.base, "main");
    assert_eq!(config.actor.default, "unknown");
    assert_eq!(config.leases.default_strength, "cooperative");
    assert_eq!(config.leases.default_intent, "other");
    assert_eq!(config.leases.default_ttl, "2h");
    assert_eq!(config.protect.mode, "guard");
    assert!(config.protect.paths.is_empty());
}

#[test]
fn config_overrides_from_toml() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join(".sv.toml");
    let toml = r#"
base = "develop"

[actor]
default = "agent-1"

[leases]
default_strength = "strong"
default_intent = "feature"
default_ttl = "30m"

[leases.compat]
allow_overlap_cooperative = false
require_flag_for_strong_overlap = false

[protect]
mode = "warn"
paths = [".beads/**", "Cargo.lock"]
"#;

    fs::write(&config_path, toml)?;

    let config = Config::load_from_repo(&dir.path().to_path_buf());

    assert_eq!(config.base, "develop");
    assert_eq!(config.actor.default, "agent-1");
    assert_eq!(config.leases.default_strength, "strong");
    assert_eq!(config.leases.default_intent, "feature");
    assert_eq!(config.leases.default_ttl, "30m");
    assert!(!config.leases.compat.allow_overlap_cooperative);
    assert!(!config.leases.compat.require_flag_for_strong_overlap);
    assert_eq!(config.protect.mode, "warn");
    assert_eq!(config.protect.paths.len(), 2);
    assert!(matches!(&config.protect.paths[0], ProtectPath::Simple(p) if p == ".beads/**"));
    assert!(matches!(&config.protect.paths[1], ProtectPath::Simple(p) if p == "Cargo.lock"));

    Ok(())
}

#[test]
fn config_load_rejects_invalid_toml() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join(".sv.toml");
    fs::write(&config_path, "this = [not valid").expect("write config");

    let result = Config::load(&config_path);
    assert!(result.is_err());
}
