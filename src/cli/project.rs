//! sv project command implementations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::actor;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::project::{ProjectRecord, ProjectStore, ProjectSyncReport};
use crate::storage::Storage;
use crate::task::TaskStore;

pub struct NewOptions {
    pub name: String,
    pub description: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ListOptions {
    pub all: bool,
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

pub struct EditOptions {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct ArchiveOptions {
    pub id: String,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

pub struct UnarchiveOptions {
    pub id: String,
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

pub struct MigrateLegacyOptions {
    pub dry_run: bool,
    pub actor: Option<String>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

#[derive(serde::Serialize)]
struct ProjectCreateOutput {
    id: String,
    name: String,
    description: Option<String>,
}

#[derive(serde::Serialize)]
struct ProjectListOutput {
    total: usize,
    projects: Vec<ProjectRecord>,
}

#[derive(serde::Serialize)]
struct ProjectChangeOutput {
    id: String,
    changed: bool,
}

#[derive(serde::Serialize)]
struct MigrateLegacyOutput {
    dry_run: bool,
    legacy_projects_found: usize,
    projects_created: usize,
    skipped_existing: usize,
    migrated: Vec<LegacyProjectMigration>,
}

#[derive(serde::Serialize)]
struct LegacyProjectMigration {
    legacy_id: String,
    project_name: String,
}

struct ProjectContext {
    project_store: ProjectStore,
    task_store: TaskStore,
    actor: Option<String>,
}

pub fn run_new(options: NewOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, false)?;
    let name = options.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidArgument(
            "project name cannot be empty".to_string(),
        ));
    }
    let description = options.description.clone();
    let id = ctx
        .project_store
        .create(name, description.clone(), ctx.actor.clone())?;
    let output = ProjectCreateOutput {
        id: id.clone(),
        name: name.to_string(),
        description,
    };
    let mut human = HumanOutput::new("Project created");
    human.push_summary("ID", id);
    human.push_summary("Name", output.name.clone());
    if let Some(description) = output.description.as_ref() {
        human.push_summary("Description", description.clone());
    }
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "project new",
        &output,
        Some(&human),
    )
}

pub fn run_list(options: ListOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let projects = ctx.project_store.list(options.all)?;
    let output = ProjectListOutput {
        total: projects.len(),
        projects,
    };
    let mut human = HumanOutput::new("Projects");
    human.push_summary("Total", output.total.to_string());
    for project in &output.projects {
        let mut line = format!("{} {}", project.id, project.name);
        if project.archived {
            line.push_str(" [archived]");
        }
        human.push_detail(line);
    }
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "project list",
        &output,
        Some(&human),
    )
}

pub fn run_show(options: ShowOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let project = ctx.project_store.get(&options.id)?;
    let mut human = HumanOutput::new(format!("Project {}", project.id));
    human.push_summary("Name", project.name.clone());
    human.push_summary("Archived", project.archived.to_string());
    if let Some(description) = project.description.as_ref() {
        human.push_summary("Description", description.clone());
    }
    human.push_summary("Created", project.created_at.to_rfc3339());
    human.push_summary("Updated", project.updated_at.to_rfc3339());
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "project show",
        &project,
        Some(&human),
    )
}

pub fn run_edit(options: EditOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, false)?;
    let changed = ctx.project_store.edit(
        &options.id,
        options.name.clone(),
        options.description.clone(),
        ctx.actor.clone(),
    )?;
    let resolved = ctx.project_store.resolve_project_id(&options.id)?;
    let output = ProjectChangeOutput {
        id: resolved.clone(),
        changed,
    };
    let mut human = HumanOutput::new(if changed {
        "Project updated".to_string()
    } else {
        "No project changes".to_string()
    });
    human.push_summary("ID", resolved);
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "project edit",
        &output,
        Some(&human),
    )
}

pub fn run_archive(options: ArchiveOptions) -> Result<()> {
    set_archived(
        options.id,
        true,
        options.actor,
        options.repo,
        options.json,
        options.quiet,
    )
}

pub fn run_unarchive(options: UnarchiveOptions) -> Result<()> {
    set_archived(
        options.id,
        false,
        options.actor,
        options.repo,
        options.json,
        options.quiet,
    )
}

pub fn run_sync(options: SyncOptions) -> Result<()> {
    let ctx = load_context(options.repo, None, false)?;
    let report: ProjectSyncReport = ctx.project_store.sync()?;
    let mut human = HumanOutput::new("Projects synced");
    human.push_summary("Events", report.total_events.to_string());
    human.push_summary("Projects", report.total_projects.to_string());
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "project sync",
        &report,
        Some(&human),
    )
}

pub fn run_migrate_legacy(options: MigrateLegacyOptions) -> Result<()> {
    let ctx = load_context(options.repo, options.actor, true)?;
    let existing: BTreeSet<String> = ctx
        .project_store
        .list(true)?
        .into_iter()
        .map(|project| project.id)
        .collect();
    let tasks = ctx.task_store.list(None)?;
    let title_by_task_id: BTreeMap<String, String> = tasks
        .iter()
        .map(|task| (task.id.clone(), task.title.clone()))
        .collect();

    let mut legacy_ids = BTreeSet::new();
    for task in &tasks {
        if let Some(project_id) = task.project.as_ref() {
            if !existing.contains(project_id) {
                legacy_ids.insert(project_id.clone());
            }
        }
    }

    let mut migrated = Vec::new();
    let mut created = 0usize;
    let mut skipped_existing = 0usize;
    for legacy_id in legacy_ids {
        if ctx
            .project_store
            .try_resolve_project_id(&legacy_id)?
            .is_some()
        {
            skipped_existing += 1;
            continue;
        }
        let name = title_by_task_id
            .get(&legacy_id)
            .cloned()
            .unwrap_or_else(|| format!("Legacy {legacy_id}"));
        if !options.dry_run {
            ctx.project_store.create_with_id(
                &legacy_id,
                &name,
                Some("migrated from task-backed project".to_string()),
                ctx.actor.clone(),
            )?;
            created += 1;
        }
        migrated.push(LegacyProjectMigration {
            legacy_id,
            project_name: name,
        });
    }

    let output = MigrateLegacyOutput {
        dry_run: options.dry_run,
        legacy_projects_found: migrated.len(),
        projects_created: created,
        skipped_existing,
        migrated,
    };
    let mut human = HumanOutput::new(if options.dry_run {
        "Legacy projects migration (dry run)".to_string()
    } else {
        "Legacy projects migrated".to_string()
    });
    human.push_summary(
        "Legacy projects found",
        output.legacy_projects_found.to_string(),
    );
    human.push_summary("Projects created", output.projects_created.to_string());
    human.push_summary("Skipped existing", output.skipped_existing.to_string());
    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "project migrate-legacy",
        &output,
        Some(&human),
    )
}

fn load_context(
    repo: Option<PathBuf>,
    actor_name: Option<String>,
    with_tasks: bool,
) -> Result<ProjectContext> {
    let repo = git::open_repo(repo.as_deref())?;
    let workdir = git::workdir(&repo)?;
    let git_dir = git::common_dir(&repo);
    let storage = Storage::new(workdir.clone(), git_dir, workdir.clone());
    let config = Config::load_from_repo(&workdir);
    let actor = actor::resolve_actor_optional(Some(&workdir), actor_name.as_deref())?;

    let task_store = if with_tasks {
        TaskStore::new(storage.clone(), config.tasks.clone())
    } else {
        TaskStore::new(storage.clone(), config.tasks)
    };
    Ok(ProjectContext {
        project_store: ProjectStore::new(storage),
        task_store,
        actor,
    })
}

fn set_archived(
    id: String,
    archived: bool,
    actor_name: Option<String>,
    repo: Option<PathBuf>,
    json: bool,
    quiet: bool,
) -> Result<()> {
    let ctx = load_context(repo, actor_name, false)?;
    let changed = ctx
        .project_store
        .set_archived(&id, archived, ctx.actor.clone())?;
    let resolved = ctx.project_store.resolve_project_id(&id)?;
    let output = ProjectChangeOutput {
        id: resolved.clone(),
        changed,
    };
    let title = if archived {
        if changed {
            "Project archived"
        } else {
            "Project already archived"
        }
    } else if changed {
        "Project unarchived"
    } else {
        "Project already active"
    };
    let mut human = HumanOutput::new(title);
    human.push_summary("ID", resolved);
    emit_success(
        OutputOptions { json, quiet },
        if archived {
            "project archive"
        } else {
            "project unarchive"
        },
        &output,
        Some(&human),
    )
}
