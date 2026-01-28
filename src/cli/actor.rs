//! sv actor command implementation
//!
//! Provides actor identity helpers (set/show).

use std::path::PathBuf;

use crate::actor;
use crate::error::Result;
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};

/// Options for `sv actor set`
pub struct SetOptions {
    pub name: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Options for `sv actor show`
pub struct ShowOptions {
    pub repo: Option<PathBuf>,
    pub actor: Option<String>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(serde::Serialize)]
struct ActorSetReport {
    actor: String,
    path: PathBuf,
}

#[derive(serde::Serialize)]
struct ActorShowReport {
    actor: String,
}

pub fn run_set(options: SetOptions) -> Result<()> {
    let start = options
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let repository = git::open_repo(Some(start.as_path()))?;
    let workdir = git::workdir(&repository)?;

    actor::persist_actor(&workdir, &options.name)?;

    let actor_name = actor::resolve_actor(Some(&workdir), Some(&options.name))?;
    let actor_path = workdir.join(".sv").join("actor");

    let report = ActorSetReport {
        actor: actor_name.clone(),
        path: actor_path.clone(),
    };

    let mut human = HumanOutput::new(format!("sv actor set: {actor_name}"));
    human.push_summary("actor", actor_name);
    human.push_summary("path", actor_path.display().to_string());
    human.push_next_step("sv status");

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "actor set",
        &report,
        Some(&human),
    )?;

    Ok(())
}

pub fn run_show(options: ShowOptions) -> Result<()> {
    let start = options
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let repository = git::open_repo(Some(start.as_path()))?;
    let workdir = git::workdir(&repository)?;

    let actor_name = actor::resolve_actor(Some(&workdir), options.actor.as_deref())?;

    let report = ActorShowReport {
        actor: actor_name.clone(),
    };

    let header = if actor_name == "unknown" {
        "sv actor: not set".to_string()
    } else {
        format!("sv actor: {actor_name}")
    };

    let mut human = HumanOutput::new(header);
    human.push_summary("actor", actor_name.clone());

    if actor_name == "unknown" {
        human.push_warning("actor not set; using default".to_string());
        human.push_next_step("sv actor set <name>");
    }

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "actor show",
        &report,
        Some(&human),
    )?;

    Ok(())
}
