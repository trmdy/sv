//! sv init command implementation
//!
//! Creates initial sv config and storage directories in a git repository.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{Error, Result};

#[derive(serde::Serialize)]
struct InitReport {
    repo_root: PathBuf,
    created_config: bool,
    created_git_sv: bool,
    created_sv_dir: bool,
    updated_gitignore: bool,
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

    if json {
        let report = InitReport {
            repo_root: workdir,
            created_config,
            created_git_sv: created_git_sv || created_oplog || created_hoist,
            created_sv_dir,
            updated_gitignore,
        };
        let payload = serde_json::to_string(&report).unwrap_or_else(|_| {
            format!(
                r#"{{"repo_root":"{}","created_config":{},"created_git_sv":{},"created_sv_dir":{},"updated_gitignore":{}}}"#,
                report.repo_root.display(),
                report.created_config,
                report.created_git_sv,
                report.created_sv_dir,
                report.updated_gitignore
            )
        });
        println!("{payload}");
        return Ok(());
    }

    if !quiet {
        let mut notes = Vec::new();
        if created_config {
            notes.push("created .sv.toml");
        }
        if created_git_sv || created_oplog || created_hoist {
            notes.push("initialized .git/sv/");
        }
        if created_sv_dir {
            notes.push("created .sv/");
        }
        if updated_gitignore {
            notes.push("updated .gitignore");
        }
        if notes.is_empty() {
            println!("sv init: nothing to do");
        } else {
            println!("sv init: {}", notes.join(", "));
        }
    }

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
