//! sv task command implementations.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::actor;
use crate::cli::ws;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::events::{Event, EventDestination, EventKind};
use crate::git;
use crate::integrations::forge as forge_integration;
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::project::ProjectStore;
use crate::storage::{Storage, WorkspaceEntry};
use crate::task::{
    CompactionPolicy, TaskDetails, TaskEvent, TaskEventType, TaskRecord, TaskRelations, TaskStore,
};

pub struct NewOptions {
    pub title: String,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub body: Option<String>,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ListOptions {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub epic: Option<String>,
    pub project: Option<String>,
    pub workspace: Option<String>,
    pub actor: Option<String>,
    pub updated_since: Option<String>,
    pub limit: Option<usize>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ReadyOptions {
    pub priority: Option<String>,
    pub epic: Option<String>,
    pub project: Option<String>,
    pub workspace: Option<String>,
    pub actor: Option<String>,
    pub updated_since: Option<String>,
    pub limit: Option<usize>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct CountOptions {
    pub ready: bool,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub epic: Option<String>,
    pub project: Option<String>,
    pub workspace: Option<String>,
    pub actor: Option<String>,
    pub updated_since: Option<String>,
    pub limit: Option<usize>,
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
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct StatusOptions {
    pub id: String,
    pub status: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct CloseOptions {
    pub id: String,
    pub status: Option<String>,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct CommentOptions {
    pub id: String,
    pub text: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ParentSetOptions {
    pub child: String,
    pub parent: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ParentClearOptions {
    pub child: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct EpicSetOptions {
    pub task: String,
    pub epic: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct EpicClearOptions {
    pub task: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ProjectSetOptions {
    pub task: String,
    pub project: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ProjectClearOptions {
    pub task: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct BlockOptions {
    pub blocker: String,
    pub blocked: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct UnblockOptions {
    pub blocker: String,
    pub blocked: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct RelateOptions {
    pub left: String,
    pub right: String,
    pub description: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct UnrelateOptions {
    pub left: String,
    pub right: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct RelationsOptions {
    pub id: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct SyncOptions {
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct PriorityOptions {
    pub id: String,
    pub priority: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct EditOptions {
    pub id: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub actor: Option<String>,
    pub events: Option<String>,
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

pub struct TuiOptions {
    pub epic: Option<String>,
    pub project: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct DeleteOptions {
    pub id: String,
    pub actor: Option<String>,
    pub events: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

struct TaskContext {
    store: TaskStore,
    actor: Option<String>,
    workspace: Option<WorkspaceEntry>,
    repo_root: PathBuf,
}

pub fn run_new(options: NewOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, false)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let title = options.title.trim();
    if title.is_empty() {
        return Err(Error::InvalidArgument("title cannot be empty".to_string()));
    }

    let status = options
        .status
        .unwrap_or_else(|| ctx.store.config().default_status.clone());
    ctx.store.validate_status(&status)?;
    let priority = match options.priority.as_deref() {
        Some(value) => ctx.store.normalize_priority(value)?,
        None => ctx.store.default_priority(),
    };

    let task_id = ctx.store.generate_task_id()?;
    let mut event = TaskEvent::new(TaskEventType::TaskCreated, task_id.clone());
    event.actor = ctx.actor.clone();
    event.title = Some(title.to_string());
    event.body = options.body;
    event.status = Some(status.clone());
    event.priority = Some(priority.clone());
    ctx.store.append_event(event.clone())?;

    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskCreated, &event);

    let output = TaskCreatedOutput {
        id: task_id.clone(),
        status: status.clone(),
        priority: priority.clone(),
    };

    let mut human = HumanOutput::new("Task created");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", task_id);
    human.push_summary("Status", status);
    human.push_summary("Priority", priority);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task new",
        &output,
        Some(&human),
    )
}

pub fn run_list(options: ListOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let updated_since = parse_timestamp("updated-since", options.updated_since.as_deref())?;
    let mut tasks = ctx.store.list(options.status.as_deref())?;
    let epic_filter = resolve_epic_filter(&ctx.store, options.epic.as_deref())?;
    let project_filter = resolve_project_filter(&ctx.store, options.project.as_deref())?;

    apply_task_filters(
        &ctx.store,
        &mut tasks,
        options.priority.as_deref(),
        epic_filter.as_deref(),
        project_filter.as_deref(),
        options.workspace.as_deref(),
        options.actor.as_deref(),
        updated_since,
    )?;

    let (blocked_ids, blocked_error) = match ctx.store.blocked_task_ids() {
        Ok(blocked_ids) => (blocked_ids, None),
        Err(err) => (
            std::collections::HashSet::new(),
            Some(format!("ready calc error: {err}")),
        ),
    };
    crate::task::sort_tasks(&mut tasks, ctx.store.config(), &blocked_ids);
    apply_limit(&mut tasks, options.limit)?;

    let output = TaskListOutput {
        total: tasks.len(),
        tasks: tasks.clone(),
    };

    let mut human = HumanOutput::new("Tasks");
    human.push_summary("Total", tasks.len().to_string());
    if let Some(epic_id) = epic_filter {
        human.push_summary("Epic", epic_id);
    }
    if let Some(project_id) = project_filter {
        human.push_summary("Project", project_id);
    }
    if let Some(error) = blocked_error {
        human.push_warning(error);
    }
    for task in tasks {
        let mut line = format!(
            "[{}][{}] {} {}",
            task.status, task.priority, task.id, task.title
        );
        if let Some(epic) = task.epic.as_ref() {
            line.push_str(&format!(" (epic: {})", epic));
        }
        if let Some(project) = task.project.as_ref() {
            line.push_str(&format!(" (project: {})", project));
        }
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

pub fn run_ready(options: ReadyOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let updated_since = parse_timestamp("updated-since", options.updated_since.as_deref())?;
    let mut tasks = ctx.store.list_ready()?;
    let epic_filter = resolve_epic_filter(&ctx.store, options.epic.as_deref())?;
    let project_filter = resolve_project_filter(&ctx.store, options.project.as_deref())?;

    apply_task_filters(
        &ctx.store,
        &mut tasks,
        options.priority.as_deref(),
        epic_filter.as_deref(),
        project_filter.as_deref(),
        options.workspace.as_deref(),
        options.actor.as_deref(),
        updated_since,
    )?;

    let blocked_ids = std::collections::HashSet::new();
    crate::task::sort_tasks(&mut tasks, ctx.store.config(), &blocked_ids);
    apply_limit(&mut tasks, options.limit)?;

    let output = TaskListOutput {
        total: tasks.len(),
        tasks: tasks.clone(),
    };

    let mut human = HumanOutput::new("Ready tasks");
    human.push_summary("Total", tasks.len().to_string());
    if let Some(epic_id) = epic_filter {
        human.push_summary("Epic", epic_id);
    }
    if let Some(project_id) = project_filter {
        human.push_summary("Project", project_id);
    }
    for task in tasks {
        let mut line = format!(
            "[{}][{}] {} {}",
            task.status, task.priority, task.id, task.title
        );
        if let Some(epic) = task.epic.as_ref() {
            line.push_str(&format!(" (epic: {})", epic));
        }
        if let Some(project) = task.project.as_ref() {
            line.push_str(&format!(" (project: {})", project));
        }
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
        "task ready",
        &output,
        Some(&human),
    )
}

#[derive(serde::Serialize)]
struct TaskCountOutput {
    total: usize,
}

pub fn run_count(options: CountOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;

    if options.ready && options.status.is_some() {
        return Err(Error::InvalidArgument(
            "cannot use --status with --ready".to_string(),
        ));
    }

    let updated_since = parse_timestamp("updated-since", options.updated_since.as_deref())?;
    let epic_filter = resolve_epic_filter(&ctx.store, options.epic.as_deref())?;
    let project_filter = resolve_project_filter(&ctx.store, options.project.as_deref())?;

    let mut tasks = if options.ready {
        ctx.store.list_ready()?
    } else {
        ctx.store.list(options.status.as_deref())?
    };

    apply_task_filters(
        &ctx.store,
        &mut tasks,
        options.priority.as_deref(),
        epic_filter.as_deref(),
        project_filter.as_deref(),
        options.workspace.as_deref(),
        options.actor.as_deref(),
        updated_since,
    )?;

    apply_limit(&mut tasks, options.limit)?;
    let output = TaskCountOutput { total: tasks.len() };

    let human = HumanOutput::new(output.total.to_string());
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task count",
        &output,
        Some(&human),
    )
}

fn apply_limit(tasks: &mut Vec<TaskRecord>, limit: Option<usize>) -> Result<()> {
    if let Some(limit) = limit {
        if limit == 0 {
            return Err(Error::InvalidArgument("limit must be >= 1".to_string()));
        }
        if tasks.len() > limit {
            tasks.truncate(limit);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_task_filters(
    store: &TaskStore,
    tasks: &mut Vec<TaskRecord>,
    priority: Option<&str>,
    epic_id: Option<&str>,
    project_id: Option<&str>,
    workspace: Option<&str>,
    actor: Option<&str>,
    updated_since: Option<DateTime<Utc>>,
) -> Result<()> {
    if let Some(priority) = priority {
        let normalized = store.normalize_priority(priority)?;
        tasks.retain(|task| task.priority == normalized);
    }

    if let Some(epic_id) = epic_id {
        tasks.retain(|task| task.id == epic_id || task.epic.as_deref() == Some(epic_id));
    }
    if let Some(project_id) = project_id {
        let effective_project = build_effective_project_map(tasks);
        tasks.retain(|task| {
            task.id == project_id
                || effective_project
                    .get(&task.id)
                    .and_then(|value| value.as_deref())
                    == Some(project_id)
        });
    }

    if let Some(actor) = actor {
        let trimmed = actor.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument("actor cannot be empty".to_string()));
        }
        tasks.retain(|task| task.updated_by.as_deref() == Some(trimmed));
    }

    if let Some(workspace) = workspace {
        let trimmed = workspace.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument(
                "workspace cannot be empty".to_string(),
            ));
        }
        let needle = trimmed.to_ascii_lowercase();
        tasks.retain(|task| {
            task.workspace_id
                .as_ref()
                .map(|value| value.eq_ignore_ascii_case(&needle))
                .unwrap_or(false)
                || task
                    .workspace
                    .as_ref()
                    .map(|value| value.eq_ignore_ascii_case(&needle))
                    .unwrap_or(false)
        });
    }

    if let Some(updated_since) = updated_since {
        tasks.retain(|task| task.updated_at >= updated_since);
    }

    Ok(())
}

fn build_effective_project_map(tasks: &[TaskRecord]) -> HashMap<String, Option<String>> {
    let mut index_by_id = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        index_by_id.insert(task.id.as_str(), idx);
    }

    let mut cache: Vec<Option<Option<String>>> = vec![None; tasks.len()];
    let mut out = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        let project =
            resolve_effective_project(idx, tasks, &index_by_id, &mut cache, &mut HashSet::new());
        out.insert(task.id.clone(), project);
    }
    out
}

fn resolve_effective_project<'a>(
    idx: usize,
    tasks: &'a [TaskRecord],
    index_by_id: &HashMap<&'a str, usize>,
    cache: &mut [Option<Option<String>>],
    visiting: &mut HashSet<usize>,
) -> Option<String> {
    if let Some(cached) = cache.get(idx).and_then(|value| value.as_ref()) {
        return cached.clone();
    }

    if !visiting.insert(idx) {
        return tasks.get(idx).and_then(|task| task.project.clone());
    }

    let resolved = if let Some(project) = tasks.get(idx).and_then(|task| task.project.clone()) {
        Some(project)
    } else if let Some(epic_idx) = tasks
        .get(idx)
        .and_then(|task| task.epic.as_deref())
        .and_then(|epic_id| index_by_id.get(epic_id))
        .copied()
    {
        resolve_effective_project(epic_idx, tasks, index_by_id, cache, visiting)
    } else {
        None
    };

    visiting.remove(&idx);
    cache[idx] = Some(resolved.clone());
    resolved
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
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;

    let workspace = ctx
        .workspace
        .ok_or_else(|| Error::OperationFailed("workspace not found for task start".to_string()))?;

    let mut event = TaskEvent::new(TaskEventType::TaskStarted, resolved.clone());
    event.actor = ctx.actor.clone();
    event.workspace_id = Some(workspace.id.clone());
    event.workspace = Some(workspace.name.clone());
    event.branch = Some(workspace.branch.clone());
    let in_progress = ctx.store.config().in_progress_status.clone();
    event.status = Some(in_progress.clone());
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskStarted, &event);
    let hook_warning = forge_integration::run_task_hook_best_effort(
        &ctx.repo_root,
        forge_integration::ForgeTaskHookKind::TaskStart,
        &resolved,
        ctx.actor.as_deref().unwrap_or("unknown"),
    );

    let output = TaskStatusOutput {
        id: resolved.clone(),
        status: in_progress,
    };

    let mut human = HumanOutput::new("Task started");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    if let Some(warning) = hook_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);
    human.push_summary("Status", output.status.clone());

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task start",
        &output,
        Some(&human),
    )
}

pub fn run_status(options: StatusOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    ctx.store.validate_status(&options.status)?;
    ensure_project_group_not_closed(&ctx.store, &resolved, &options.status)?;

    let mut event = TaskEvent::new(TaskEventType::TaskStatusChanged, resolved.clone());
    event.actor = ctx.actor.clone();
    event.status = Some(options.status.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskStatusChanged, &event);

    let output = TaskStatusOutput {
        id: resolved.clone(),
        status: options.status.clone(),
    };

    let mut human = HumanOutput::new("Task status updated");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);
    human.push_summary("Status", output.status.clone());

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task status",
        &output,
        Some(&human),
    )
}

pub fn run_priority(options: PriorityOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    let priority = ctx.store.normalize_priority(&options.priority)?;

    let mut event = TaskEvent::new(TaskEventType::TaskPriorityChanged, resolved.clone());
    event.actor = ctx.actor.clone();
    event.priority = Some(priority.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskPriorityChanged, &event);

    let output = TaskPriorityOutput {
        id: resolved.clone(),
        priority: priority.clone(),
    };

    let mut human = HumanOutput::new("Task priority updated");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);
    human.push_summary("Priority", priority);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task priority",
        &output,
        Some(&human),
    )
}

pub fn run_edit(options: EditOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;

    if options.title.is_none() && options.body.is_none() {
        return Err(Error::InvalidArgument(
            "task edit requires --title or --body".to_string(),
        ));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskEdited, resolved.clone());
    event.actor = ctx.actor.clone();
    if let Some(title) = options.title.as_ref() {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument("title cannot be empty".to_string()));
        }
        event.title = Some(trimmed.to_string());
    }
    if let Some(body) = options.body.as_ref() {
        event.body = Some(body.clone());
    }
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskEdited, &event);

    let output = TaskEditOutput {
        id: resolved.clone(),
        title: event.title.clone(),
        body: event.body.clone(),
    };

    let mut human = HumanOutput::new("Task updated");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);
    if let Some(title) = output.title.as_ref() {
        human.push_summary("Title", title.clone());
    }
    if let Some(body) = output.body.as_ref() {
        let label = if body.trim().is_empty() {
            "(cleared)".to_string()
        } else {
            body.clone()
        };
        human.push_summary("Body", label);
    }

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task edit",
        &output,
        Some(&human),
    )
}

pub fn run_close(options: CloseOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
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
    ensure_project_group_not_closed(&ctx.store, &resolved, &status)?;

    let mut event = TaskEvent::new(TaskEventType::TaskClosed, resolved.clone());
    event.actor = ctx.actor.clone();
    event.status = Some(status.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskClosed, &event);
    let hook_warning = forge_integration::run_task_hook_best_effort(
        &ctx.repo_root,
        forge_integration::ForgeTaskHookKind::TaskClose,
        &resolved,
        ctx.actor.as_deref().unwrap_or("unknown"),
    );

    let output = TaskStatusOutput {
        id: resolved.clone(),
        status: status.clone(),
    };

    let mut human = HumanOutput::new("Task closed");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    if let Some(warning) = hook_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);
    human.push_summary("Status", status);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task close",
        &output,
        Some(&human),
    )
}

pub fn run_delete(options: DeleteOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;

    let details = ctx.store.details(&resolved)?;
    let relations = &details.relations;
    let has_relations = details.task.epic.is_some()
        || !relations.epic_tasks.is_empty()
        || details.task.project.is_some()
        || !relations.project_tasks.is_empty()
        || relations.parent.is_some()
        || !relations.children.is_empty()
        || !relations.blocks.is_empty()
        || !relations.blocked_by.is_empty()
        || !relations.relates.is_empty();
    if has_relations {
        return Err(Error::InvalidArgument(format!(
            "task has relations; clear them first (sv task relations {resolved})"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskDeleted, resolved.clone());
    event.actor = ctx.actor.clone();
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskDeleted, &event);

    let output = TaskDeleteOutput {
        id: resolved.clone(),
    };

    let mut human = HumanOutput::new("Task deleted");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task delete",
        &output,
        Some(&human),
    )
}

pub fn run_comment(options: CommentOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    let text = options.text.trim();
    if text.is_empty() {
        return Err(Error::InvalidArgument(
            "comment cannot be empty".to_string(),
        ));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskCommented, resolved.clone());
    event.actor = ctx.actor.clone();
    event.comment = Some(text.to_string());
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskCommented, &event);

    let output = TaskCommentOutput {
        id: resolved.clone(),
        comment: text.to_string(),
    };

    let mut human = HumanOutput::new("Comment added");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("ID", resolved);
    human.push_summary("Comment", text.to_string());

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task comment",
        &output,
        Some(&human),
    )
}

pub fn run_parent_set(options: ParentSetOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let child = ctx.store.resolve_task_id(&options.child)?;
    let parent = ctx.store.resolve_task_id(&options.parent)?;
    if child == parent {
        return Err(Error::InvalidArgument(
            "parent cannot match child".to_string(),
        ));
    }

    let relations = ctx.store.relations(&child)?;
    if relations.parent.as_deref() == Some(parent.as_str()) {
        return Err(Error::InvalidArgument(format!(
            "parent already set to {parent}"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskParentSet, child.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(parent.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskParentSet, &event);

    let output = TaskParentOutput {
        child: child.clone(),
        parent: parent.clone(),
    };

    let mut human = HumanOutput::new("Parent set");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Child", child);
    human.push_summary("Parent", parent);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task parent set",
        &output,
        Some(&human),
    )
}

pub fn run_parent_clear(options: ParentClearOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let child = ctx.store.resolve_task_id(&options.child)?;
    let relations = ctx.store.relations(&child)?;
    let parent = relations
        .parent
        .ok_or_else(|| Error::InvalidArgument(format!("task has no parent: {child}")))?;

    let mut event = TaskEvent::new(TaskEventType::TaskParentCleared, child.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(parent.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskParentCleared, &event);

    let output = TaskParentOutput {
        child: child.clone(),
        parent: parent.clone(),
    };

    let mut human = HumanOutput::new("Parent cleared");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Child", child);
    human.push_summary("Parent", parent);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task parent clear",
        &output,
        Some(&human),
    )
}

pub fn run_epic_set(options: EpicSetOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let task = ctx.store.resolve_task_id(&options.task)?;
    let epic = ctx.store.resolve_task_id(&options.epic)?;
    if task == epic {
        return Err(Error::InvalidArgument("epic cannot match task".to_string()));
    }

    let details = ctx.store.details(&task)?;
    if details.task.epic.as_deref() == Some(epic.as_str()) {
        return Err(Error::InvalidArgument(format!(
            "epic already set to {epic}"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskEpicSet, task.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(epic.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskEpicSet, &event);

    let output = TaskEpicOutput {
        task: task.clone(),
        epic: epic.clone(),
    };

    let mut human = HumanOutput::new("Epic set");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Task", task);
    human.push_summary("Epic", epic);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task epic set",
        &output,
        Some(&human),
    )
}

pub fn run_epic_clear(options: EpicClearOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let task = ctx.store.resolve_task_id(&options.task)?;
    let details = ctx.store.details(&task)?;
    let epic = details
        .task
        .epic
        .ok_or_else(|| Error::InvalidArgument(format!("task has no epic: {task}")))?;

    let mut event = TaskEvent::new(TaskEventType::TaskEpicCleared, task.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(epic.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskEpicCleared, &event);

    let output = TaskEpicOutput {
        task: task.clone(),
        epic: epic.clone(),
    };

    let mut human = HumanOutput::new("Epic cleared");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Task", task);
    human.push_summary("Epic", epic);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task epic clear",
        &output,
        Some(&human),
    )
}

pub fn run_project_set(options: ProjectSetOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let task = ctx.store.resolve_task_id(&options.task)?;
    let project_target = resolve_project_target(&ctx.store, &options.project)?;
    let project = project_target.id().to_string();
    if let ProjectTarget::Entity(ref project_id) = project_target {
        let project_store = ProjectStore::new(ctx.store.storage().clone());
        let project_record = project_store.get(project_id)?;
        if project_record.archived {
            return Err(Error::InvalidArgument(format!(
                "project is archived: {project_id}"
            )));
        }
    }
    if matches!(project_target, ProjectTarget::LegacyTask(_)) && task == project {
        return Err(Error::InvalidArgument(
            "project cannot match task".to_string(),
        ));
    }

    let details = ctx.store.details(&task)?;
    if details.task.project.as_deref() == Some(project.as_str()) {
        return Err(Error::InvalidArgument(format!(
            "project already set to {project}"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskProjectSet, task.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(project.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskProjectSet, &event);

    let output = TaskProjectOutput {
        task: task.clone(),
        project: project.clone(),
    };

    let mut human = HumanOutput::new("Project set");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Task", task);
    human.push_summary("Project", project);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task project set",
        &output,
        Some(&human),
    )
}

enum ProjectTarget {
    Entity(String),
    LegacyTask(String),
}

impl ProjectTarget {
    fn id(&self) -> &str {
        match self {
            ProjectTarget::Entity(id) | ProjectTarget::LegacyTask(id) => id.as_str(),
        }
    }
}

fn resolve_project_target(store: &TaskStore, input: &str) -> Result<ProjectTarget> {
    let project_store = ProjectStore::new(store.storage().clone());
    if let Some(project_id) = project_store.try_resolve_project_id(input)? {
        return Ok(ProjectTarget::Entity(project_id));
    }
    Ok(ProjectTarget::LegacyTask(store.resolve_task_id(input)?))
}

pub fn run_project_clear(options: ProjectClearOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let task = ctx.store.resolve_task_id(&options.task)?;
    let details = ctx.store.details(&task)?;
    let project = details
        .task
        .project
        .ok_or_else(|| Error::InvalidArgument(format!("task has no project: {task}")))?;

    let mut event = TaskEvent::new(TaskEventType::TaskProjectCleared, task.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(project.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskProjectCleared, &event);

    let output = TaskProjectOutput {
        task: task.clone(),
        project: project.clone(),
    };

    let mut human = HumanOutput::new("Project cleared");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Task", task);
    human.push_summary("Project", project);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task project clear",
        &output,
        Some(&human),
    )
}

pub fn run_block(options: BlockOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let blocker = ctx.store.resolve_task_id(&options.blocker)?;
    let blocked = ctx.store.resolve_task_id(&options.blocked)?;
    if blocker == blocked {
        return Err(Error::InvalidArgument(
            "blocked task cannot match blocker".to_string(),
        ));
    }

    let relations = ctx.store.relations(&blocker)?;
    if relations.blocks.iter().any(|id| id == &blocked) {
        return Err(Error::InvalidArgument(format!(
            "task already blocks {blocked}"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskBlocked, blocker.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(blocked.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskBlocked, &event);
    let hook_warning = forge_integration::run_task_hook_best_effort(
        &ctx.repo_root,
        forge_integration::ForgeTaskHookKind::TaskBlock,
        &blocker,
        ctx.actor.as_deref().unwrap_or("unknown"),
    );

    let output = TaskBlockOutput {
        blocker: blocker.clone(),
        blocked: blocked.clone(),
    };

    let mut human = HumanOutput::new("Task blocked");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    if let Some(warning) = hook_warning {
        human.push_warning(warning);
    }
    human.push_summary("Blocker", blocker);
    human.push_summary("Blocked", blocked);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task block",
        &output,
        Some(&human),
    )
}

pub fn run_unblock(options: UnblockOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let blocker = ctx.store.resolve_task_id(&options.blocker)?;
    let blocked = ctx.store.resolve_task_id(&options.blocked)?;
    if blocker == blocked {
        return Err(Error::InvalidArgument(
            "blocked task cannot match blocker".to_string(),
        ));
    }

    let relations = ctx.store.relations(&blocker)?;
    if !relations.blocks.iter().any(|id| id == &blocked) {
        return Err(Error::InvalidArgument(format!(
            "task does not block {blocked}"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskUnblocked, blocker.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(blocked.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskUnblocked, &event);

    let output = TaskBlockOutput {
        blocker: blocker.clone(),
        blocked: blocked.clone(),
    };

    let mut human = HumanOutput::new("Task unblocked");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Blocker", blocker);
    human.push_summary("Blocked", blocked);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task unblock",
        &output,
        Some(&human),
    )
}

pub fn run_relate(options: RelateOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let left = ctx.store.resolve_task_id(&options.left)?;
    let right = ctx.store.resolve_task_id(&options.right)?;
    if left == right {
        return Err(Error::InvalidArgument(
            "related task cannot match source".to_string(),
        ));
    }
    let description = options.description.trim();
    if description.is_empty() {
        return Err(Error::InvalidArgument(
            "relation description cannot be empty".to_string(),
        ));
    }

    let relations = ctx.store.relations(&left)?;
    if let Some(existing) = relations.relates.iter().find(|rel| rel.id == right) {
        if existing.description == description {
            return Err(Error::InvalidArgument(format!(
                "relation already exists for {right}"
            )));
        }
    }

    let mut event = TaskEvent::new(TaskEventType::TaskRelated, left.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(right.clone());
    event.relation_description = Some(description.to_string());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskRelated, &event);

    let output = TaskRelateOutput {
        left: left.clone(),
        right: right.clone(),
        description: description.to_string(),
    };

    let mut human = HumanOutput::new("Tasks related");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Left", left);
    human.push_summary("Right", right);
    human.push_summary("Description", description.to_string());

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task relate",
        &output,
        Some(&human),
    )
}

pub fn run_unrelate(options: UnrelateOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let (mut event_sink, events_to_stdout) = open_task_event_sink(options.events.as_deref())?;
    let left = ctx.store.resolve_task_id(&options.left)?;
    let right = ctx.store.resolve_task_id(&options.right)?;
    if left == right {
        return Err(Error::InvalidArgument(
            "related task cannot match source".to_string(),
        ));
    }

    let relations = ctx.store.relations(&left)?;
    if !relations.relates.iter().any(|rel| rel.id == right) {
        return Err(Error::InvalidArgument(format!(
            "relation not found for {right}"
        )));
    }

    let mut event = TaskEvent::new(TaskEventType::TaskUnrelated, left.clone());
    event.actor = ctx.actor.clone();
    event.related_task_id = Some(right.clone());
    if let Some(workspace) = ctx.workspace.as_ref() {
        event.workspace_id = Some(workspace.id.clone());
        event.workspace = Some(workspace.name.clone());
        event.branch = Some(workspace.branch.clone());
    }
    ctx.store.append_event(event.clone())?;
    let event_warning = emit_task_event(&mut event_sink, EventKind::TaskUnrelated, &event);

    let output = TaskUnrelateOutput {
        left: left.clone(),
        right: right.clone(),
    };

    let mut human = HumanOutput::new("Tasks unrelated");
    if let Some(warning) = event_warning {
        human.push_warning(warning);
    }
    human.push_summary("Left", left);
    human.push_summary("Right", right);

    emit_success(
        OutputOptions {
            json: options.json && !events_to_stdout,
            quiet: options.quiet || events_to_stdout,
        },
        "task unrelate",
        &output,
        Some(&human),
    )
}

pub fn run_relations(options: RelationsOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let resolved = ctx.store.resolve_task_id(&options.id)?;
    let relations = ctx.store.relations(&resolved)?;

    let output = TaskRelationsOutput {
        id: resolved.clone(),
        relations: relations.clone(),
    };

    let mut human = HumanOutput::new(format!("Relations for {resolved}"));
    if let Some(parent) = relations.parent.as_ref() {
        human.push_summary("Parent", parent.clone());
    }
    if !relations.children.is_empty() {
        human.push_summary("Children", relations.children.join(", "));
    }
    if !relations.blocks.is_empty() {
        human.push_summary("Blocks", relations.blocks.join(", "));
    }
    if !relations.blocked_by.is_empty() {
        human.push_summary("Blocked by", relations.blocked_by.join(", "));
    }
    if !relations.relates.is_empty() {
        for relation in relations.relates {
            human.push_detail(format!(
                "Relates: {} ({})",
                relation.id, relation.description
            ));
        }
    }

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "task relations",
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

pub fn run_tui(options: TuiOptions) -> Result<()> {
    if options.json {
        return Err(Error::InvalidArgument(
            "task TUI does not support --json".to_string(),
        ));
    }
    if options.quiet {
        return Err(Error::InvalidArgument(
            "task TUI does not support --quiet".to_string(),
        ));
    }
    let ctx = load_context(options.repo, None, false)?;
    let epic_filter = resolve_epic_filter(&ctx.store, options.epic.as_deref())?;
    let project_filter = resolve_project_filter(&ctx.store, options.project.as_deref())?;
    crate::ui::task_viewer::run(ctx.store, epic_filter, project_filter)
}

#[derive(serde::Serialize)]
struct TaskCreatedOutput {
    id: String,
    status: String,
    priority: String,
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
struct TaskPriorityOutput {
    id: String,
    priority: String,
}

#[derive(serde::Serialize)]
struct TaskEditOutput {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
}

#[derive(serde::Serialize)]
struct TaskCommentOutput {
    id: String,
    comment: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn task(id: &str) -> TaskRecord {
        let now = Utc::now();
        TaskRecord {
            id: id.to_string(),
            title: "Title".to_string(),
            status: "open".to_string(),
            priority: "P2".to_string(),
            created_at: now,
            updated_at: now,
            created_by: None,
            updated_by: None,
            body: None,
            epic: None,
            project: None,
            workspace_id: None,
            workspace: None,
            branch: None,
            started_at: None,
            started_by: None,
            closed_at: None,
            closed_by: None,
            comments_count: 0,
            last_comment_at: None,
        }
    }

    #[test]
    fn apply_limit_truncates() {
        let mut tasks = vec![task("a"), task("b"), task("c")];
        apply_limit(&mut tasks, Some(2)).expect("limit");
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn apply_limit_rejects_zero() {
        let mut tasks = vec![task("a")];
        assert!(apply_limit(&mut tasks, Some(0)).is_err());
    }
}

#[derive(serde::Serialize)]
struct TaskParentOutput {
    child: String,
    parent: String,
}

#[derive(serde::Serialize)]
struct TaskEpicOutput {
    task: String,
    epic: String,
}

#[derive(serde::Serialize)]
struct TaskProjectOutput {
    task: String,
    project: String,
}

#[derive(serde::Serialize)]
struct TaskBlockOutput {
    blocker: String,
    blocked: String,
}

#[derive(serde::Serialize)]
struct TaskRelateOutput {
    left: String,
    right: String,
    description: String,
}

#[derive(serde::Serialize)]
struct TaskUnrelateOutput {
    left: String,
    right: String,
}

#[derive(serde::Serialize)]
struct TaskDeleteOutput {
    id: String,
}

#[derive(serde::Serialize)]
struct TaskRelationsOutput {
    id: String,
    relations: TaskRelations,
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

#[derive(serde::Serialize)]
struct TaskEventData {
    id: String,
    event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    related_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relation_description: Option<String>,
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
        repo_root: workdir,
    })
}

fn parse_timestamp(label: &str, value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|err| {
        Error::InvalidArgument(format!("invalid {label} timestamp '{value}': {err}"))
    })?;
    Ok(Some(parsed.with_timezone(&Utc)))
}

fn status_is_closed(store: &TaskStore, status: &str) -> bool {
    store
        .config()
        .closed_statuses
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(status.trim()))
}

fn ensure_project_group_not_closed(store: &TaskStore, task_id: &str, status: &str) -> Result<()> {
    if !status_is_closed(store, status) {
        return Ok(());
    }
    let relations = store.relations(task_id)?;
    if relations.project_tasks.is_empty() {
        return Ok(());
    }
    Err(Error::InvalidArgument(
        "project groups cannot be completed; close member tasks instead".to_string(),
    ))
}

fn resolve_epic_filter(store: &TaskStore, value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidArgument("epic cannot be empty".to_string()));
    }
    Ok(Some(store.resolve_task_id(trimmed)?))
}

fn resolve_project_filter(store: &TaskStore, value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidArgument(
            "project cannot be empty".to_string(),
        ));
    }
    let project_store = ProjectStore::new(store.storage().clone());
    if let Some(project_id) = project_store.try_resolve_project_id(trimmed)? {
        return Ok(Some(project_id));
    }
    Ok(Some(store.resolve_task_id(trimmed)?))
}

fn open_task_event_sink(events: Option<&str>) -> Result<(Option<crate::events::EventSink>, bool)> {
    let destination = EventDestination::parse(events);
    let sink = destination.as_ref().map(|dest| dest.open()).transpose()?;
    let events_to_stdout = matches!(destination, Some(EventDestination::Stdout));
    Ok((sink, events_to_stdout))
}

fn emit_task_event(
    sink: &mut Option<crate::events::EventSink>,
    kind: EventKind,
    event: &TaskEvent,
) -> Option<String> {
    let sink = sink.as_mut()?;

    let mut envelope = Event::new(kind, event.actor.clone());
    envelope.timestamp = event.timestamp;
    let envelope = match envelope.with_data(task_event_data(event)) {
        Ok(envelope) => envelope,
        Err(err) => return Some(format!("event output failed: {err}")),
    };

    if let Err(err) = sink.emit(&envelope) {
        return Some(format!("event output failed: {err}"));
    }

    None
}

fn task_event_data(event: &TaskEvent) -> TaskEventData {
    TaskEventData {
        id: event.task_id.clone(),
        event_id: event.event_id.clone(),
        actor: event.actor.clone(),
        title: event.title.clone(),
        body: event.body.clone(),
        status: event.status.clone(),
        priority: event.priority.clone(),
        workspace_id: event.workspace_id.clone(),
        workspace: event.workspace.clone(),
        branch: event.branch.clone(),
        comment: event.comment.clone(),
        related_task_id: event.related_task_id.clone(),
        relation_description: event.relation_description.clone(),
    }
}

fn push_task_summary(human: &mut HumanOutput, details: &TaskDetails) {
    let task = &details.task;
    human.push_summary("Title", task.title.clone());
    human.push_summary("Status", task.status.clone());
    human.push_summary("Priority", task.priority.clone());
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
    if let Some(epic) = task.epic.as_ref() {
        human.push_summary("Epic", epic.clone());
    }
    if !details.relations.epic_tasks.is_empty() {
        human.push_summary("Epic tasks", details.relations.epic_tasks.join(", "));
    }
    if let Some(project) = task.project.as_ref() {
        human.push_summary("Project", project.clone());
    }
    if !details.relations.project_tasks.is_empty() {
        human.push_summary("Project tasks", details.relations.project_tasks.join(", "));
    }
    if let Some(parent) = details.relations.parent.as_ref() {
        human.push_summary("Parent", parent.clone());
    }
    if !details.relations.children.is_empty() {
        human.push_summary("Children", details.relations.children.join(", "));
    }
    if !details.relations.blocks.is_empty() {
        human.push_summary("Blocks", details.relations.blocks.join(", "));
    }
    if !details.relations.blocked_by.is_empty() {
        human.push_summary("Blocked by", details.relations.blocked_by.join(", "));
    }
    if !details.relations.relates.is_empty() {
        for relation in &details.relations.relates {
            human.push_detail(format!(
                "Relates: {} ({})",
                relation.id, relation.description
            ));
        }
    }
}
