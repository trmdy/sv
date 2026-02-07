use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};

pub struct HooksInstallOptions {
    pub loop_ref: Option<String>,
    pub status_map: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(serde::Serialize)]
struct HooksInstallOutput {
    config_path: String,
    loop_ref: String,
    status_open: String,
    status_blocked: String,
    status_closed: String,
}

pub fn run_hooks_install(options: HooksInstallOptions) -> Result<()> {
    let repo = git::open_repo(options.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let config_path = workdir.join(".sv.toml");

    let loop_ref = options.loop_ref.unwrap_or_else(|| "{actor}".to_string());
    if loop_ref.trim().is_empty() {
        return Err(Error::InvalidArgument("--loop cannot be empty".to_string()));
    }

    let status_map = parse_status_map(options.status_map.as_deref())?;
    let status_open = status_map.open.unwrap_or_else(|| "in_progress".to_string());
    let status_blocked = status_map.blocked.unwrap_or_else(|| "blocked".to_string());
    let status_closed = status_map.closed.unwrap_or_else(|| "done".to_string());

    let mut doc = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            toml::from_str::<toml::Value>(&content)?
        }
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    set_value(
        &mut doc,
        &["integrations", "forge", "enabled"],
        toml::Value::Boolean(true),
    );
    set_value(
        &mut doc,
        &["integrations", "forge", "loop_ref"],
        toml::Value::String(loop_ref.clone()),
    );

    let on_start_cmd = format!(
        "forge work set {{task_id}} --status {status_open} --loop {{loop_ref}} --agent {{actor}}"
    );
    set_value(
        &mut doc,
        &["integrations", "forge", "on_task_start", "cmd"],
        toml::Value::String(on_start_cmd),
    );

    let on_block_cmd = format!(
        "forge work set {{task_id}} --status {status_blocked} --loop {{loop_ref}} --agent {{actor}}"
    );
    set_value(
        &mut doc,
        &["integrations", "forge", "on_task_block", "cmd"],
        toml::Value::String(on_block_cmd),
    );

    let on_close_cmd = format!(
        "forge work set {{task_id}} --status {status_closed} --loop {{loop_ref}} --agent {{actor}} && forge work clear --loop {{loop_ref}}"
    );
    set_value(
        &mut doc,
        &["integrations", "forge", "on_task_close", "cmd"],
        toml::Value::String(on_close_cmd),
    );

    let mut rendered = toml::to_string_pretty(&doc)?;
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    std::fs::write(&config_path, rendered)?;

    let output = HooksInstallOutput {
        config_path: config_path.display().to_string(),
        loop_ref: loop_ref.clone(),
        status_open: status_open.clone(),
        status_blocked: status_blocked.clone(),
        status_closed: status_closed.clone(),
    };

    let mut human = HumanOutput::new("Forge hooks installed");
    human.push_summary("Config", output.config_path.clone());
    human.push_summary("Loop ref", output.loop_ref.clone());
    human.push_summary("Hooks", "task start, task block, task close".to_string());
    human.push_next_step("sv task start <id>");

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "forge hooks install",
        &output,
        Some(&human),
    )
}

#[derive(Default)]
struct StatusMap {
    open: Option<String>,
    blocked: Option<String>,
    closed: Option<String>,
}

fn parse_status_map(raw: Option<&str>) -> Result<StatusMap> {
    let Some(raw) = raw else {
        return Ok(StatusMap::default());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(StatusMap::default());
    }

    let mut map = StatusMap::default();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = part.split_once('=').ok_or_else(|| {
            Error::InvalidArgument(format!(
                "invalid --status-map entry '{part}' (expected key=value)"
            ))
        })?;
        let key = k.trim();
        let value = v.trim();
        if value.is_empty() {
            return Err(Error::InvalidArgument(format!(
                "invalid --status-map entry '{part}' (empty value)"
            )));
        }

        match key {
            "open" => map.open = Some(value.to_string()),
            "blocked" => map.blocked = Some(value.to_string()),
            "closed" => map.closed = Some(value.to_string()),
            _ => {
                return Err(Error::InvalidArgument(format!(
                    "invalid --status-map key '{key}' (expected open|blocked|closed)"
                )))
            }
        }
    }
    Ok(map)
}

fn set_value(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }

    if !root.is_table() {
        *root = toml::Value::Table(toml::map::Map::new());
    }

    let mut current = root;
    for key in &path[..path.len() - 1] {
        if !current.is_table() {
            *current = toml::Value::Table(toml::map::Map::new());
        }
        let table = current.as_table_mut().expect("table");
        current = table
            .entry((*key).to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }

    let last_key = path[path.len() - 1].to_string();
    let table = current.as_table_mut().expect("table");
    table.insert(last_key, value);
}
