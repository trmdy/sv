//! sv task command implementations.

use std::path::PathBuf;

use crate::actor;
use crate::cli::ws;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::storage::{Storage, WorkspaceEntry};
use crate::task::{CompactionPolicy, TaskDetails, TaskEvent, TaskEventType, TaskRecord, TaskStore};


pub struct NewOptions {
    pub title: String,
    pub status: Option<String>,
    pub body: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ListOptions {
    pub status: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ShowOptions {
    pub id: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct StartOptions {
    pub id: String,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct StatusOptions {
    pub id: String,
    pub status: String,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct CloseOptions {
    pub id: String,
    pub status: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct CommentOptions {
    pub id: String,
    pub text: String,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct SyncOptions {
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct CompactOptions {
    pub older_than: Option<String>,
    pub max_log_mb: Option<u64>,
    pub dry_run: bool,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct PrefixOptions {
    pub prefix: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

struct TaskContext {
    store: TaskStore,
    actor: Option<String>,
    workspace: Option<WorkspaceEntry>,
}

pub fn run_new(options: NewOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, false)?;
    let title = options.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidArgument("title cannot be empty".to_string()));
    }

    let status = options
        .status
        .unwrap_or_else(|| ctx.store.config().default_status.clone());
    ctx.store.validate_status(&status)?;

    let task_id = ctx.store.generate_task_id()?;
    let mut event = TaskEvent::new(TaskEventType::TaskCreated, task_id.clone());
    event.actor = ctx.actor.clone();
    event.title = Some(title.to_string());
    event.body = options.body;
    event.status = Some(status.clone());
    ctx.store.append_event(event)?;

    let output = TaskCreatedOutput {
        id: task_id.clone(),
        status: status.clone(),
    };

    let mut human = HumanOutput::new("Task created");
    human.push_summary("ID", task_id);
    human.push_summary("Status", status);

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task new",
        &output,
        Some(&human),
    )
}

pub fn run_list(options: ListOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let tasks = ctx.store.list(options.status.as_deref())?;

    let output = TaskListOutput {
        total: tasks.len(),
        tasks: tasks.clone(),
    };

    let mut human = HumanOutput::new("Tasks");
    human.push_summary("Total", tasks.len().to_string());
    for task in tasks {
        let mut line = format!("[{}] {} {}", task.status, task.id, task.title);
        if let Some(workspace) = task.workspace.as_ref() {
            line.push_str(&format!(" (ws: {})", workspace));
        }
        human.push_detail(line);
    }

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task list",
        &output,
        Some(&human),
    )
}

pub fn run_show(options: ShowOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    let details = ctx.store.details(&resolved)?;

    let mut human = HumanOutput::new(format!("Task {}", resolved));
    push_task_summary(&mut human, &details);
    for comment in &details.comments {
        let actor = comment.actor.as_deref().unwrap_or("unknown");
        human.push_detail(format!(
            "[{}] {}: {}",
            comment.timestamp.to_rfc3339(),
            actor,
            comment.comment
        ));
    }

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task show",
        &details,
        Some(&human),
    )
}

pub fn run_start(options: StartOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;

    let workspace = ctx.workspace.ok_or_else(|| {
        Error::OperationFailed("workspace not found for task start".to_string())
    })?;

    let mut event = TaskEvent::new(TaskEventType::TaskStarted, resolved.clone());
    event.actor = ctx.actor.clone();
    event.workspace_id = Some(workspace.id.clone());
    event.workspace = Some(workspace.name.clone());
    event.branch = Some(workspace.branch.clone());
    ctx.store.append_event(event)?;

    let output = TaskStatusOutput {
        id: resolved.clone(),
        status: ctx.store.config().in_progress_status.clone(),
    };

    let mut human = HumanOutput::new("Task started");
    human.push_summary("ID", resolved);
    human.push_summary("Status", output.status.clone());

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task start",
        &output,
        Some(&human),
    )
}

pub fn run_status(options: StatusOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    ctx.store.validate_status(&options.status)?;

    let mut event = TaskEvent::new(TaskEventType::TaskStatusChanged, resolved.clone());
    event.actor = ctx.actor.clone();
    event.status = Some(options.status.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event)?;

    let output = TaskStatusOutput {
        id: resolved.clone(),
        status: options.status.clone(),
    };

    let mut human = HumanOutput::new("Task status updated");
    human.push_summary("ID", resolved);
    human.push_summary("Status", output.status.clone());

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task status",
        &output,
        Some(&human),
    )
}

pub fn run_close(options: CloseOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;

    let status = options.status.unwrap_or_else(|| {
        ctx.store
            .config()
            .closed_statuses
            .first()
            .cloned()
            .unwrap_or_else(|| "closed".to_string())
    });
    ctx.store.validate_status(&status)?;

    let mut event = TaskEvent::new(TaskEventType::TaskClosed, resolved.clone());
    event.actor = ctx.actor.clone();
    event.status = Some(status.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event)?;

    let output = TaskStatusOutput {
        id: resolved.clone(),
        status: status.clone(),
    };

    let mut human = HumanOutput::new("Task closed");
    human.push_summary("ID", resolved);
    human.push_summary("Status", status);

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task close",
        &output,
        Some(&human),
    )
}

pub fn run_comment(options: CommentOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    let text = options.text.trim();
    if text.is_empty() {
        return Err(Error::InvalidArgument("comment cannot be empty".to_string()));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskCommented, resolved.clone());
    event.actor = ctx.actor.clone();
    event.comment = Some(text.to_string());
    ctx.store.append_event(event)?;

    let output = TaskCommentOutput {
        id: resolved.clone(),
        comment: text.to_string(),
    };

    let mut human = HumanOutput::new("Comment added");
    human.push_summary("ID", resolved);
    human.push_summary("Comment", text.to_string());

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task comment",
        &output,
        Some(&human),
    )
}

pub fn run_sync(options: SyncOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let policy = ctx.store.auto_compaction_policy()?;
    let report = ctx.store.sync(policy)?;

    let mut human = HumanOutput::new("Task sync complete");
    human.push_summary("Events", report.total_events.to_string());
    human.push_summary("Tasks", report.total_tasks.to_string());
    if report.compacted {
        human.push_summary("Compacted", report.removed_events.to_string());
    }

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task sync",
        &report,
        Some(&human),
    )
}

pub fn run_compact(options: CompactOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;

    if let Some(max_log_mb) = options.max_log_mb {
        let size_mb = ctx
            .store
            .tracked_log_path()
            .metadata()
            .ok()
            .map(|meta| meta.len() / (1024 * 1024))
            .unwrap_or(0);
        if size_mb < max_log_mb {
            let report = TaskCompactOutput {
                before_events: 0,
                after_events: 0,
                removed_events: 0,
                compacted_tasks: 0,
            };
            let human = HumanOutput::new("No compaction needed");
            return emit_success(
                OutputOptions {
                    json: options.json,
                    quiet: options.quiet,
                },
                "task compact",
                &report,
                Some(&human),
            );
        }
    }

    let older_than = match options.older_than {
        Some(value) => Some(crate::lease::parse_duration(&value)?),
        None => None,
    };
    let policy = CompactionPolicy {
        older_than,
        max_log_mb: options.max_log_mb,
    };
    let (events, report) = ctx.store.compact(policy)?;

    if !options.dry_run {
        ctx.store.replace_events(&events)?;
    }

    let output = TaskCompactOutput {
        before_events: report.before_events,
        after_events: report.after_events,
        removed_events: report.removed_events,
        compacted_tasks: report.compacted_tasks,
    };

    let mut human = HumanOutput::new("Task compaction complete");
    human.push_summary("Before", report.before_events.to_string());
    human.push_summary("After", report.after_events.to_string());
    human.push_summary("Removed", report.removed_events.to_string());
    human.push_summary("Compacted tasks", report.compacted_tasks.to_string());

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task compact",
        &output,
        Some(&human),
    )
}

pub fn run_prefix(options: PrefixOptions) -> Result<()> {
    let repo = git::open_repo(options.repo.as_deref())?;
    let workdir = git::workdir(&repo)?;

    let config_path = workdir.join(".sv.toml");
    let mut config = if config_path.exists() {
        Config::load(&config_path)?
    } else {
        Config::default()
    };

    let mut updated = false;
    if let Some(prefix) = options.prefix {
        let trimmed = prefix.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument("prefix cannot be empty".to_string()));
        }
        if !trimmed.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return Err(Error::InvalidArgument(
                "prefix must be alphanumeric".to_string(),
            ));
        }
        config.tasks.id_prefix = trimmed.to_string();
        config.save(&config_path)?;
        updated = true;
    }

    let output = TaskPrefixOutput {
        prefix: config.tasks.id_prefix.clone(),
        updated,
    };

    let header = if updated {
        "Task prefix set"
    } else {
        "Task prefix"
    };
    let mut human = HumanOutput::new(header);
    human.push_summary("Prefix", config.tasks.id_prefix.clone());

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task prefix",
        &output,
        Some(&human),
    )
}

#[derive(serde::Serialize)]
struct TaskCreatedOutput {
    id: String,
    status: String,
}

#[derive(serde::Serialize)]
struct TaskListOutput {
    total: usize,
    tasks: Vec<TaskRecord>,
}

#[derive(serde::Serialize)]
struct TaskStatusOutput {
    id: String,
    status: String,
}

#[derive(serde::Serialize)]
struct TaskCommentOutput {
    id: String,
    comment: String,
}

#[derive(serde::Serialize)]
struct TaskCompactOutput {
    before_events: usize,
    after_events: usize,
    removed_events: usize,
    compacted_tasks: usize,
}

#[derive(serde::Serialize)]
struct TaskPrefixOutput {
    prefix: String,
    updated: bool,
}

fn load_context(
    repo: Option<PathBuf>,
    actor: Option<String>,
    ensure_workspace: bool,
) -> Result<TaskContext> {
    let repo = git::open_repo(repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = git::common_dir(&repo);
    let storage = Storage::new(workdir.clone(), git_dir, workdir.clone());
    let config = Config::load_from_repo(&workdir);
    let store = TaskStore::new(storage, config.tasks);
    let actor = actor::resolve_actor_optional(Some(&workdir), actor.as_deref())?;

    let workspace = if ensure_workspace {
        Some(ws::ensure_current_workspace(
            store.storage(),
            &repo,
            &workdir,
            actor.as_deref(),
        )?)
    } else {
        None
    };

    Ok(TaskContext {
        store,
        actor,
        workspace,
    })
}

fn push_task_summary(human: &mut HumanOutput, details: &TaskDetails) {
    let task = &details.task;
    human.push_summary("Title", task.title.clone());
    human.push_summary("Status", task.status.clone());
    human.push_summary("Created", task.created_at.to_rfc3339());
    human.push_summary("Updated", task.updated_at.to_rfc3339());
    if let Some(workspace) = task.workspace.as_ref() {
        human.push_summary("Workspace", workspace.clone());
    }
    if let Some(branch) = task.branch.as_ref() {
        human.push_summary("Branch", branch.clone());
    }
    if let Some(body) = task.body.as_ref() {
        human.push_detail(body.clone());
    }
}
