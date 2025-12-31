//! sv op subcommand implementations.
//!
//! Provides operation log display with filtering.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::git;
use crate::oplog::{format_records, OpLog, OpLogFilter, OpOutcome};
use crate::storage::Storage;

/// Options for the op log command.
pub struct LogOptions {
    pub limit: usize,
    pub actor: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub operation: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(Serialize)]
struct LogEntry {
    op_id: String,
    timestamp: String,
    actor: Option<String>,
    command: String,
    affected_refs: Vec<String>,
    affected_workspaces: Vec<String>,
    outcome: OpOutcome,
}

#[derive(Serialize)]
struct LogReport {
    records: Vec<LogEntry>,
    total: usize,
}

/// Run the op log command.
pub fn run_log(options: LogOptions) -> Result<()> {
    let repo = git::open_repo(options.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let common_dir = resolve_common_dir(&repo)?;

    let storage = Storage::new(workdir.clone(), common_dir, workdir);
    if !storage.is_initialized() {
        return Err(Error::OperationFailed(
            "sv not initialized. Run 'sv init' first.".to_string(),
        ));
    }

    let since = parse_timestamp("since", options.since.as_deref())?;
    let until = parse_timestamp("until", options.until.as_deref())?;

    let filter = OpLogFilter {
        actor: options.actor.clone(),
        since,
        until,
        operation: options.operation.clone(),
    };

    let log = OpLog::for_storage(&storage);
    let records = log.read_filtered(&filter, Some(options.limit))?;

    if options.json {
        let entries: Vec<LogEntry> = records
            .iter()
            .map(|record| LogEntry {
                op_id: record.op_id.to_string(),
                timestamp: record.timestamp.to_rfc3339(),
                actor: record.actor.clone(),
                command: record.command.clone(),
                affected_refs: record.affected_refs.clone(),
                affected_workspaces: record.affected_workspaces.clone(),
                outcome: record.outcome.clone(),
            })
            .collect();

        let report = LogReport {
            total: entries.len(),
            records: entries,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if options.quiet {
        return Ok(());
    }

    if records.is_empty() {
        println!("No operations recorded.");
    } else {
        println!("{}", format_records(&records));
    }

    Ok(())
}

fn parse_timestamp(label: &str, value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|err| {
        Error::InvalidArgument(format!(
            "invalid {label} timestamp '{value}': {err}"
        ))
    })?;
    Ok(Some(parsed.with_timezone(&Utc)))
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
