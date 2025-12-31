use std::fs;

use sv::config::Config;

#[test]
fn load_from_repo_defaults_on_invalid_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(".sv.toml");
    fs::write(&path, "base = 123").expect("write invalid config");

    let cfg = Config::load_from_repo(&dir.path().to_path_buf());
    assert_eq!(cfg.base, "main");
    assert_eq!(cfg.actor.default, "unknown");
}

#[test]
fn load_from_repo_defaults_on_invalid_protect_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(".sv.toml");
    let content = r#"
[protect]
mode = "bogus"
"#;
    fs::write(&path, content.trim()).expect("write invalid protect mode");

    let cfg = Config::load_from_repo(&dir.path().to_path_buf());
    assert_eq!(cfg.protect.mode, "guard");
}
