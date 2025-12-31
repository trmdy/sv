//! sv init command implementation
//!
//! Creates initial sv config and storage directories in a git repository.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::output::{emit_success, HumanOutput, OutputOptions};

#[derive(serde::Serialize)]
struct InitReport {
    repo: PathBuf,
    created: InitCreated,
    updated: InitUpdated,
}

#[derive(serde::Serialize)]
struct InitCreated {
    config: bool,
    sv_dir: bool,
    git_sv: bool,
}

#[derive(serde::Serialize)]
struct InitUpdated {
    gitignore: bool,
}

pub fn run(repo: Option<PathBuf>, json: bool, quiet: bool) -> Result<()> {
    let start = match repo {
        Some(path) => path,
        None => std::env::current_dir()?,
    };

    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;

    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?
        .to_path_buf();

    let common_dir = resolve_common_dir(&repository)?;

    let (created_git_sv, created_oplog, created_hoist) =
        ensure_git_sv_dirs(&common_dir)?;
    let created_sv_dir = ensure_dir(&workdir.join(".sv"))?;
    let created_config = ensure_config(&workdir)?;
    let updated_gitignore = ensure_gitignore(&workdir)?;

    let created_git_sv = created_git_sv || created_oplog || created_hoist;

    let report = InitReport {
        repo: workdir.clone(),
        created: InitCreated {
            config: created_config,
            sv_dir: created_sv_dir,
            git_sv: created_git_sv,
        },
        updated: InitUpdated {
            gitignore: updated_gitignore,
        },
    };

    let mut created_items = Vec::new();
    if created_config {
        created_items.push(".sv.toml");
    }
    if created_sv_dir {
        created_items.push(".sv/");
    }
    if created_git_sv {
        created_items.push(".git/sv/");
    }

    let mut updated_items = Vec::new();
    if updated_gitignore {
        updated_items.push(".gitignore");
    }

    let header = if created_items.is_empty() && updated_items.is_empty() {
        "sv init: nothing to do".to_string()
    } else {
        "sv init: initialized repo".to_string()
    };

    let mut human = HumanOutput::new(header);
    human.push_summary("repo", workdir.display().to_string());
    human.push_summary(
        "created",
        if created_items.is_empty() {
            "none".to_string()
        } else {
            created_items.join(", ")
        },
    );
    human.push_summary(
        "updated",
        if updated_items.is_empty() {
            "none".to_string()
        } else {
            updated_items.join(", ")
        },
    );
    human.push_next_step("sv actor set <name>");
    human.push_next_step("sv ws new <workspace>");

    emit_success(
        OutputOptions { json, quiet },
        "init",
        &report,
        Some(&human),
    )?;

    Ok(())
}

fn ensure_git_sv_dirs(common_dir: &Path) -> Result<(bool, bool, bool)> {
    let base = common_dir.join("sv");
    let created_base = ensure_dir(&base)?;
    let created_oplog = ensure_dir(&base.join("oplog"))?;
    let created_hoist = ensure_dir(&base.join("hoist"))?;
    Ok((created_base, created_oplog, created_hoist))
}

fn resolve_common_dir(repository: &git2::Repository) -> Result<PathBuf> {
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

fn ensure_config(repo_root: &Path) -> Result<bool> {
    let config_path = repo_root.join(".sv.toml");
    if config_path.exists() {
        if !config_path.is_file() {
            return Err(Error::OperationFailed(format!(
                ".sv.toml exists but is not a file: {}",
                config_path.display()
            )));
        }
        return Ok(false);
    }

    let config = Config::default();
    config.save(&config_path)?;
    Ok(true)
}

fn ensure_gitignore(repo_root: &Path) -> Result<bool> {
    let path = repo_root.join(".gitignore");
    if path.exists() && !path.is_file() {
        return Err(Error::OperationFailed(format!(
            ".gitignore exists but is not a file: {}",
            path.display()
        )));
    }

    let existing = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };

    if has_sv_ignore(&existing) {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(".sv/\n");
    std::fs::write(&path, updated)?;
    Ok(true)
}

fn has_sv_ignore(contents: &str) -> bool {
    contents.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }
        trimmed == ".sv"
            || trimmed == ".sv/"
            || trimmed.starts_with(".sv/")
            || trimmed.starts_with(".sv\\")
    })
}

fn ensure_dir(path: &Path) -> Result<bool> {
    if path.exists() {
        if !path.is_dir() {
            return Err(Error::OperationFailed(format!(
                "Expected directory at {}",
                path.display()
            )));
        }
        return Ok(false);
    }

    std::fs::create_dir_all(path)?;
    Ok(true)
}
