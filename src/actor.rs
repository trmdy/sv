//! Actor identity management.
//!
//! Actor resolution order:
//! 1) CLI --actor (explicit)
//! 2) SV_ACTOR environment variable
//! 3) Persisted workspace value in .sv/actor
//! 4) Config default (actor.default) or "unknown"

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{Error, Result};

const ACTOR_FILENAME: &str = "actor";

/// Resolve the current actor using CLI, environment, persisted value, and config.
pub fn resolve_actor(repo_root: Option<&Path>, cli_actor: Option<&str>) -> Result<String> {
    if let Some(actor) = non_empty(cli_actor) {
        return Ok(actor.to_string());
    }

    if let Ok(env_actor) = std::env::var("SV_ACTOR") {
        if let Some(actor) = non_empty(Some(env_actor.as_str())) {
            return Ok(actor.to_string());
        }
    }

    if let Some(root) = repo_root {
        if let Some(actor) = load_persisted_actor(root)? {
            return Ok(actor);
        }

        let config = Config::load_from_repo(&root.to_path_buf());
        return Ok(config.actor.default);
    }

    Ok("unknown".to_string())
}

/// Resolve the current actor, returning None when it resolves to "unknown".
pub fn resolve_actor_optional(
    repo_root: Option<&Path>,
    cli_actor: Option<&str>,
) -> Result<Option<String>> {
    let actor = resolve_actor(repo_root, cli_actor)?;
    if actor == "unknown" {
        Ok(None)
    } else {
        Ok(Some(actor))
    }
}

/// Persist the actor identity in `.sv/actor`.
pub fn persist_actor(repo_root: &Path, actor: &str) -> Result<()> {
    let actor = non_empty(Some(actor))
        .ok_or_else(|| Error::InvalidArgument("actor name cannot be empty".to_string()))?;

    let sv_dir = repo_root.join(".sv");
    std::fs::create_dir_all(&sv_dir)?;
    let path = actor_path(repo_root);
    std::fs::write(path, format!("{actor}\n"))?;
    Ok(())
}

/// Load the actor identity from `.sv/actor`, if present.
pub fn load_persisted_actor(repo_root: &Path) -> Result<Option<String>> {
    let path = actor_path(repo_root);
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)?;
    let actor = raw.trim();
    if actor.is_empty() {
        return Ok(None);
    }

    Ok(Some(actor.to_string()))
}

fn actor_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".sv").join(ACTOR_FILENAME)
}

fn non_empty(input: Option<&str>) -> Option<&str> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn write_config(repo_root: &Path, default_actor: &str) {
        let config = format!(
            "[actor]\ndefault = \"{}\"\n",
            default_actor.replace('\"', "\\\"")
        );
        fs::write(repo_root.join(".sv.toml"), config).expect("write config");
    }

    fn write_actor_file(repo_root: &Path, actor: &str) {
        let sv_dir = repo_root.join(".sv");
        fs::create_dir_all(&sv_dir).expect("create .sv");
        fs::write(sv_dir.join("actor"), format!("{actor}\n")).expect("write actor file");
    }

    #[test]
    fn resolve_actor_prefers_cli_env_persisted_config() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        write_config(dir.path(), "config-actor");
        write_actor_file(dir.path(), "persisted-actor");

        let _env = EnvGuard::set("SV_ACTOR", "env-actor");
        let actor = resolve_actor(Some(dir.path()), Some("cli-actor")).expect("resolve");
        assert_eq!(actor, "cli-actor");

        let actor = resolve_actor(Some(dir.path()), None).expect("resolve");
        assert_eq!(actor, "env-actor");

        drop(_env);
        let _env = EnvGuard::remove("SV_ACTOR");
        let actor = resolve_actor(Some(dir.path()), None).expect("resolve");
        assert_eq!(actor, "persisted-actor");

        fs::remove_file(dir.path().join(".sv/actor")).expect("remove actor file");
        let actor = resolve_actor(Some(dir.path()), None).expect("resolve");
        assert_eq!(actor, "config-actor");
    }

    #[test]
    fn persist_actor_rejects_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = persist_actor(dir.path(), "   ").expect_err("should reject empty");
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn load_persisted_actor_handles_missing_or_blank() {
        let dir = tempfile::tempdir().expect("tempdir");
        let actor = load_persisted_actor(dir.path()).expect("load");
        assert!(actor.is_none());

        write_actor_file(dir.path(), "");
        let actor = load_persisted_actor(dir.path()).expect("load");
        assert!(actor.is_none());
    }

    #[test]
    fn resolve_actor_optional_returns_none_for_unknown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let actor = resolve_actor_optional(Some(dir.path()), None).expect("resolve");
        assert!(actor.is_none());
    }
}
