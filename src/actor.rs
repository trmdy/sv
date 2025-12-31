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

/// Persist the actor identity in `.sv/actor`.
pub fn persist_actor(repo_root: &Path, actor: &str) -> Result<()> {
    let actor = non_empty(Some(actor)).ok_or_else(|| {
        Error::InvalidArgument("actor name cannot be empty".to_string())
    })?;

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
